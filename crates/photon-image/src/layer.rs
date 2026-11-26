//! Multi-layer raster stack, blend modes, transparency masks, and CPU compositor for `photon-image`.

use std::sync::Arc;

use image::{Rgba, RgbaImage};
use photon_protocol::{
    BlendMode, CanvasAnchor, LayerEffect, LayerKind, LayerSummary, LayerTransformSpec,
    TextLayerData, TransformEachSpec, UiText, VectorBlendSpec, VectorGeometry, VectorLayerData,
    VectorPathNode, VectorPathOperation, VectorShape,
};

use crate::ImageError;
use crate::selection::SelectionMask;
use crate::text::rasterize_text_layer;
use crate::vector::{
    add_node_at_closest_point, add_node_on_segment, apply_blend_to_layer,
    apply_transform_each_to_layer, convert_shape_to_path, delete_node, geometry_to_bezier_path,
    join_shapes, move_node_anchor, move_node_handle, render_vector_layer, reverse_shape_path,
    set_node_mode, set_path_closed, simplify_shape, smooth_shape, split_shape_at_node,
    toggle_path_closed, transform_shape_once,
};

/// One raster, vector, or text layer in a document stack.
///
/// For vector and text layers, `pixels` holds the cached rasterization at canvas resolution
/// so all blend modes, masks, effects, thumbnails, and export pipelines work seamlessly.
#[derive(Debug, Clone)]
pub struct Layer {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub offset_x: i32,
    pub offset_y: i32,
    pub pixels: Arc<RgbaImage>,
    /// Layer-local baseline pixels used by `LiquifyMode::Reconstruct`.
    pub original: Option<Arc<RgbaImage>>,
    /// Optional non-destructive 8-bit grayscale transparency mask matching
    /// `pixels.dimensions()` (`255` = revealed, `0` = hidden).
    pub mask: Option<Arc<Vec<u8>>>,
    /// Whether `mask` is applied during compositing.
    pub mask_enabled: bool,
    /// Whether paint/erase strokes target `mask` instead of `pixels`.
    pub editing_mask: bool,
    /// When `true`, moving the layer moves its mask together.
    pub mask_linked: bool,
    pub mask_offset_x: i32,
    pub mask_offset_y: i32,
    pub effect: Option<LayerEffect>,
    /// Non-destructive vector shapes when this is a [`LayerKind::Vector`] layer.
    pub vector_data: Option<VectorLayerData>,
    /// Non-destructive editable text specification when this is a [`LayerKind::Text`] layer.
    pub text_data: Option<TextLayerData>,
}

impl Layer {
    pub fn new(id: u64, name: impl Into<String>, pixels: Arc<RgbaImage>) -> Self {
        Self {
            id,
            name: name.into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            offset_x: 0,
            offset_y: 0,
            original: Some(Arc::clone(&pixels)),
            pixels,
            mask: None,
            mask_enabled: true,
            editing_mask: false,
            mask_linked: true,
            mask_offset_x: 0,
            mask_offset_y: 0,
            effect: None,
            vector_data: None,
            text_data: None,
        }
    }

    /// Construct a [`LayerKind::Vector`] layer and populate its cached `pixels` buffer.
    pub fn new_vector(
        id: u64,
        name: impl Into<String>,
        canvas_width: u32,
        canvas_height: u32,
        vector_data: VectorLayerData,
    ) -> Self {
        let rendered = Arc::new(render_vector_layer(
            &vector_data,
            canvas_width,
            canvas_height,
        ));
        let mut layer = Self::new(id, name, rendered);
        layer.vector_data = Some(vector_data);
        layer
    }

    /// Construct a [`LayerKind::Text`] layer and populate its cached `pixels` buffer.
    pub fn new_text(
        id: u64,
        name: impl Into<String>,
        canvas_width: u32,
        canvas_height: u32,
        text_data: TextLayerData,
    ) -> Self {
        let rendered = Arc::new(rasterize_text_layer(
            &text_data,
            canvas_width,
            canvas_height,
        ));
        let mut layer = Self::new(id, name, rendered);
        layer.text_data = Some(text_data);
        layer
    }

    pub fn kind(&self) -> LayerKind {
        if self.vector_data.is_some() {
            LayerKind::Vector
        } else if self.text_data.is_some() {
            LayerKind::Text
        } else {
            LayerKind::Raster
        }
    }

    pub fn is_raster(&self) -> bool {
        self.kind() == LayerKind::Raster
    }

    pub fn is_vector(&self) -> bool {
        self.vector_data.is_some()
    }

    pub fn is_text(&self) -> bool {
        self.text_data.is_some()
    }

    /// Re-render cached `pixels` from `vector_data` or `text_data` at `(canvas_width, canvas_height)`.
    pub fn refresh_cached_pixels(&mut self, canvas_width: u32, canvas_height: u32) {
        if let Some(ref vdata) = self.vector_data {
            let next = Arc::new(render_vector_layer(vdata, canvas_width, canvas_height));
            self.original = Some(Arc::clone(&next));
            self.pixels = next;
        } else if let Some(ref tdata) = self.text_data {
            let next = Arc::new(rasterize_text_layer(tdata, canvas_width, canvas_height));
            self.original = Some(Arc::clone(&next));
            self.pixels = next;
        }
    }

    /// Convert a [`LayerKind::Vector`] or [`LayerKind::Text`] layer into a permanent
    /// [`LayerKind::Raster`] layer while keeping its rendered `pixels`.
    /// Returns `true` if the layer was previously vector or text.
    pub fn rasterize(&mut self, canvas_width: u32, canvas_height: u32) -> bool {
        if self.vector_data.is_none() && self.text_data.is_none() {
            return false;
        }
        self.refresh_cached_pixels(canvas_width, canvas_height);
        self.vector_data = None;
        self.text_data = None;
        true
    }

    pub fn dimensions(&self) -> (u32, u32) {
        self.pixels.dimensions()
    }

    pub fn is_empty(&self) -> bool {
        !self.pixels.pixels().any(|p| p.0[3] > 0)
    }

    pub fn summary(&self) -> LayerSummary {
        let (width, height) = self.dimensions();
        LayerSummary {
            id: self.id,
            name: self.name.clone(),
            kind: self.kind(),
            visible: self.visible,
            locked: self.locked,
            opacity: self.opacity,
            blend_mode: self.blend_mode,
            offset_x: self.offset_x,
            offset_y: self.offset_y,
            width,
            height,
            has_mask: self.mask.is_some(),
            mask_enabled: self.mask_enabled,
            editing_mask: self.editing_mask,
            mask_linked: self.mask_linked,
            effect: self.effect.clone(),
        }
    }
}

/// Convert a vector or text [`Layer`] in-place into a [`LayerKind::Raster`] layer.
pub fn rasterize_layer(layer: &mut Layer, canvas_width: u32, canvas_height: u32) -> bool {
    layer.rasterize(canvas_width, canvas_height)
}

/// Ordered stack of raster layers sharing a document canvas.
#[derive(Debug, Clone)]
pub struct LayerStack {
    pub canvas_width: u32,
    pub canvas_height: u32,
    /// Bottom-to-top order (`layers[0]` is background, `layers.last()` is topmost).
    pub layers: Vec<Layer>,
    pub active_layer_id: u64,
    pub next_id: u64,
    /// Optional solid RGB backdrop composited underneath all layers (`None` = transparent).
    pub background: Option<[u8; 3]>,
    /// Non-destructive 8-bit pixel selection mask matching `canvas_width × canvas_height`.
    pub selection: SelectionMask,
}

/// Structural or property mutation applied to a [`LayerStack`].
#[derive(Debug, Clone, PartialEq)]
pub enum LayerOperation {
    AddEmpty {
        name: String,
    },
    AddImage {
        name: String,
        offset_x: i32,
        offset_y: i32,
        pixels: Arc<RgbaImage>,
    },
    Duplicate {
        id: u64,
    },
    Delete {
        id: u64,
    },
    Reorder {
        id: u64,
        new_index: usize,
    },
    SetProps {
        id: u64,
        name: Option<String>,
        visible: Option<bool>,
        locked: Option<bool>,
        opacity: Option<f32>,
        blend_mode: Option<BlendMode>,
        offset_x: Option<i32>,
        offset_y: Option<i32>,
    },
    MergeDown {
        id: u64,
    },
    Flatten,
    SetCanvasBackground {
        color: Option<[u8; 3]>,
    },
    AddMask {
        id: u64,
        reveal_all: bool,
    },
    DeleteMask {
        id: u64,
    },
    ApplyMask {
        id: u64,
    },
    InvertMask {
        id: u64,
    },
    SetMaskEnabled {
        id: u64,
        enabled: bool,
    },
    SetEditingMask {
        id: u64,
        editing: bool,
    },
    SetMaskLinked {
        id: u64,
        linked: bool,
    },
    AddEffect {
        name: String,
        effect: LayerEffect,
    },
    UpdateEffect {
        id: u64,
        effect: LayerEffect,
    },
    AddVectorLayer {
        name: String,
        data: VectorLayerData,
    },
    UpdateVectorLayer {
        id: u64,
        data: VectorLayerData,
    },
    AddVectorShape {
        layer_id: u64,
        shape: VectorShape,
    },
    UpdateVectorShape {
        layer_id: u64,
        shape: VectorShape,
    },
    UpdateVectorPathNodes {
        layer_id: u64,
        shape_id: u64,
        nodes: Vec<VectorPathNode>,
        closed: Option<bool>,
    },
    DeleteVectorShape {
        layer_id: u64,
        shape_id: u64,
    },
    ConvertShapeToPath {
        layer_id: u64,
        shape_id: u64,
    },
    VectorPathOp {
        layer_id: u64,
        shape_id: Option<u64>,
        op: VectorPathOperation,
    },
    TransformEach {
        layer_ids: Vec<u64>,
        shape_ids: Vec<u64>,
        spec: TransformEachSpec,
    },
    VectorBlend {
        layer_id: u64,
        spec: VectorBlendSpec,
    },
    AddTextLayer {
        name: String,
        data: TextLayerData,
    },
    UpdateTextLayer {
        id: u64,
        data: TextLayerData,
    },
    RasterizeLayer {
        id: u64,
    },
}

impl LayerOperation {
    pub fn ui_label(&self) -> UiText {
        match self {
            Self::AddEmpty { name } | Self::AddImage { name, .. } => {
                let display = if name.trim().is_empty() {
                    "Layer".to_owned()
                } else {
                    name.trim().to_owned()
                };
                UiText::with("op_layer_add", [("name", display)])
            }
            Self::AddVectorLayer { name, .. } => {
                let display = if name.trim().is_empty() {
                    "Vector Layer".to_owned()
                } else {
                    name.trim().to_owned()
                };
                UiText::with("op_layer_add", [("name", display)])
            }
            Self::AddTextLayer { name, data } => {
                let display = if !name.trim().is_empty() {
                    name.trim().to_owned()
                } else if !data.text.trim().is_empty() {
                    data.text.lines().next().unwrap_or("Text").trim().to_owned()
                } else {
                    "Text Layer".to_owned()
                };
                UiText::with("op_layer_add", [("name", display)])
            }
            Self::AddEffect { name, effect } => {
                let display = if name.trim().is_empty() {
                    effect.default_name().to_owned()
                } else {
                    name.trim().to_owned()
                };
                UiText::with("op_layer_add", [("name", display)])
            }
            Self::UpdateEffect { .. }
            | Self::UpdateVectorLayer { .. }
            | Self::AddVectorShape { .. }
            | Self::UpdateVectorShape { .. }
            | Self::UpdateVectorPathNodes { .. }
            | Self::DeleteVectorShape { .. }
            | Self::ConvertShapeToPath { .. }
            | Self::VectorPathOp { .. }
            | Self::TransformEach { .. }
            | Self::VectorBlend { .. }
            | Self::UpdateTextLayer { .. }
            | Self::RasterizeLayer { .. } => UiText::new("op_layer_props"),
            Self::Duplicate { .. } => UiText::new("op_layer_duplicate"),
            Self::Delete { .. } => UiText::new("op_layer_delete"),
            Self::Reorder { .. } => UiText::new("op_layer_reorder"),
            Self::SetProps { .. }
            | Self::SetMaskEnabled { .. }
            | Self::SetEditingMask { .. }
            | Self::SetMaskLinked { .. } => UiText::new("op_layer_props"),
            Self::MergeDown { .. } => UiText::new("op_layer_merge_down"),
            Self::Flatten => UiText::new("op_layer_flatten"),
            Self::SetCanvasBackground { .. } => UiText::new("op_canvas_bg"),
            Self::AddMask { .. } => UiText::new("op_mask_add"),
            Self::DeleteMask { .. } => UiText::new("op_mask_delete"),
            Self::ApplyMask { .. } => UiText::new("op_mask_apply"),
            Self::InvertMask { .. } => UiText::new("op_mask_invert"),
        }
    }
}

impl LayerStack {
    pub fn from_single(image: Arc<RgbaImage>) -> Self {
        let (canvas_width, canvas_height) = image.dimensions();
        let bg = Layer::new(1, "Background", image);
        Self {
            canvas_width,
            canvas_height,
            layers: vec![bg],
            active_layer_id: 1,
            next_id: 2,
            background: None,
            selection: SelectionMask::empty(canvas_width, canvas_height),
        }
    }

    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }

    pub fn active_index(&self) -> usize {
        self.index_of(self.active_layer_id).unwrap_or(0)
    }

    pub fn active_layer(&self) -> &Layer {
        &self.layers[self.active_index()]
    }

    pub fn active_layer_mut(&mut self) -> &mut Layer {
        let idx = self.active_index();
        &mut self.layers[idx]
    }

    pub fn summaries(&self) -> Vec<LayerSummary> {
        self.layers.iter().map(Layer::summary).collect()
    }

    pub fn select(&mut self, id: u64) -> Result<bool, ImageError> {
        if self.index_of(id).is_none() {
            return Err(ImageError::coded("layer_not_found"));
        }
        if self.active_layer_id == id {
            return Ok(false);
        }
        self.active_layer_id = id;
        Ok(true)
    }

    /// Add a new [`LayerKind::Vector`] layer right above the active layer and select it.
    pub fn add_vector_layer(&mut self, name: impl Into<String>, data: VectorLayerData) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let raw_name = name.into();
        let label = if raw_name.trim().is_empty() {
            format!("Vector {id}")
        } else {
            raw_name.trim().to_owned()
        };
        let layer = Layer::new_vector(id, label, self.canvas_width, self.canvas_height, data);
        let insert_at = (self.active_index() + 1).min(self.layers.len());
        self.layers.insert(insert_at, layer);
        self.active_layer_id = id;
        id
    }

    /// Add a new [`LayerKind::Text`] layer right above the active layer and select it.
    pub fn add_text_layer(&mut self, name: impl Into<String>, data: TextLayerData) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let raw_name = name.into();
        let label = if !raw_name.trim().is_empty() {
            raw_name.trim().to_owned()
        } else if !data.text.trim().is_empty() {
            data.text.lines().next().unwrap_or("Text").trim().to_owned()
        } else {
            format!("Text {id}")
        };
        let layer = Layer::new_text(id, label, self.canvas_width, self.canvas_height, data);
        let insert_at = (self.active_index() + 1).min(self.layers.len());
        self.layers.insert(insert_at, layer);
        self.active_layer_id = id;
        id
    }

    /// Re-render cached `pixels` for a vector or text layer by `id`.
    pub fn refresh_layer_pixels(&mut self, id: u64) -> Result<bool, ImageError> {
        let Some(idx) = self.index_of(id) else {
            return Err(ImageError::coded("layer_not_found"));
        };
        if self.layers[idx].is_raster() {
            return Ok(false);
        }
        let (cw, ch) = (self.canvas_width, self.canvas_height);
        self.layers[idx].refresh_cached_pixels(cw, ch);
        Ok(true)
    }

    /// Convert a vector or text layer into a permanent raster layer by `id`.
    pub fn rasterize_layer(&mut self, id: u64) -> Result<bool, ImageError> {
        let Some(idx) = self.index_of(id) else {
            return Err(ImageError::coded("layer_not_found"));
        };
        if self.layers[idx].locked {
            return Err(ImageError::coded("layer_locked"));
        }
        let (cw, ch) = (self.canvas_width, self.canvas_height);
        Ok(self.layers[idx].rasterize(cw, ch))
    }

    pub fn apply_op(&mut self, op: &LayerOperation) -> Result<bool, ImageError> {
        match op {
            LayerOperation::AddEmpty { name } => {
                let id = self.next_id;
                self.next_id += 1;
                let blank = Arc::new(RgbaImage::from_pixel(
                    self.canvas_width.max(1),
                    self.canvas_height.max(1),
                    Rgba([0, 0, 0, 0]),
                ));
                let label = if name.trim().is_empty() {
                    format!("Layer {id}")
                } else {
                    name.trim().to_owned()
                };
                let insert_at = (self.active_index() + 1).min(self.layers.len());
                self.layers.insert(insert_at, Layer::new(id, label, blank));
                self.active_layer_id = id;
                Ok(true)
            }
            LayerOperation::AddImage {
                name,
                offset_x,
                offset_y,
                pixels,
            } => {
                let id = self.next_id;
                self.next_id += 1;
                let label = if name.trim().is_empty() {
                    format!("Layer {id}")
                } else {
                    name.trim().to_owned()
                };
                let mut layer = Layer::new(id, label, Arc::clone(pixels));
                layer.offset_x = *offset_x;
                layer.offset_y = *offset_y;
                let insert_at = (self.active_index() + 1).min(self.layers.len());
                self.layers.insert(insert_at, layer);
                self.active_layer_id = id;
                Ok(true)
            }
            LayerOperation::Duplicate { id } => {
                let Some(idx) = self.index_of(*id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                let new_id = self.next_id;
                self.next_id += 1;
                let mut copy = self.layers[idx].clone();
                copy.id = new_id;
                copy.name = format!("{} copy", copy.name);
                copy.locked = false;
                self.layers.insert(idx + 1, copy);
                self.active_layer_id = new_id;
                Ok(true)
            }
            LayerOperation::Delete { id } => {
                if self.layers.len() <= 1 {
                    return Err(ImageError::coded("cannot_delete_last_layer"));
                }
                let Some(idx) = self.index_of(*id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                self.layers.remove(idx);
                if self.active_layer_id == *id {
                    let next_idx = idx.saturating_sub(1).min(self.layers.len() - 1);
                    self.active_layer_id = self.layers[next_idx].id;
                }
                Ok(true)
            }
            LayerOperation::Reorder { id, new_index } => {
                let Some(old_idx) = self.index_of(*id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                let clamped = (*new_index).min(self.layers.len() - 1);
                if old_idx == clamped {
                    return Ok(false);
                }
                let layer = self.layers.remove(old_idx);
                self.layers.insert(clamped, layer);
                Ok(true)
            }
            LayerOperation::SetProps {
                id,
                name,
                visible,
                locked,
                opacity,
                blend_mode,
                offset_x,
                offset_y,
            } => {
                let Some(idx) = self.index_of(*id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                let layer = &mut self.layers[idx];
                let mut changed = false;
                if let Some(l) = *locked
                    && layer.locked != l
                {
                    layer.locked = l;
                    changed = true;
                }
                if let Some(n) = name {
                    let trimmed = n.trim();
                    if !trimmed.is_empty() && layer.name != trimmed {
                        layer.name = trimmed.to_owned();
                        changed = true;
                    }
                }
                if let Some(v) = *visible
                    && layer.visible != v
                {
                    layer.visible = v;
                    changed = true;
                }
                if let Some(op) = *opacity {
                    let c = op.clamp(0.0, 1.0);
                    if (layer.opacity - c).abs() > 1e-4 {
                        layer.opacity = c;
                        changed = true;
                    }
                }
                if let Some(bm) = *blend_mode
                    && layer.blend_mode != bm
                {
                    layer.blend_mode = bm;
                    changed = true;
                }
                if let Some(ox) = *offset_x {
                    if layer.locked {
                        return Err(ImageError::coded("layer_locked"));
                    }
                    let dx = ox - layer.offset_x;
                    if dx != 0 {
                        if layer.mask.is_some() && !layer.mask_linked && layer.editing_mask {
                            layer.mask_offset_x += dx;
                        } else {
                            layer.offset_x = ox;
                            if layer.mask.is_some() && !layer.mask_linked {
                                layer.mask_offset_x -= dx;
                            }
                        }
                        changed = true;
                    }
                }
                if let Some(oy) = *offset_y {
                    if layer.locked {
                        return Err(ImageError::coded("layer_locked"));
                    }
                    let dy = oy - layer.offset_y;
                    if dy != 0 {
                        if layer.mask.is_some() && !layer.mask_linked && layer.editing_mask {
                            layer.mask_offset_y += dy;
                        } else {
                            layer.offset_y = oy;
                            if layer.mask.is_some() && !layer.mask_linked {
                                layer.mask_offset_y -= dy;
                            }
                        }
                        changed = true;
                    }
                }
                Ok(changed)
            }
            LayerOperation::MergeDown { id } => {
                let Some(idx) = self.index_of(*id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if idx == 0 {
                    return Err(ImageError::coded("cannot_merge_bottom_layer"));
                }
                if self.layers[idx].locked || self.layers[idx - 1].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                let merged = composite_pair(
                    self.canvas_width,
                    self.canvas_height,
                    &self.layers[idx - 1],
                    &self.layers[idx],
                );
                let target = &mut self.layers[idx - 1];
                target.pixels = Arc::new(merged);
                target.original = Some(Arc::clone(&target.pixels));
                target.vector_data = None;
                target.text_data = None;
                target.mask = None;
                target.mask_enabled = true;
                target.editing_mask = false;
                target.mask_linked = true;
                target.mask_offset_x = 0;
                target.mask_offset_y = 0;
                target.offset_x = 0;
                target.offset_y = 0;
                target.opacity = 1.0;
                target.blend_mode = BlendMode::Normal;
                target.visible = true;
                let kept_id = target.id;
                self.layers.remove(idx);
                self.active_layer_id = kept_id;
                Ok(true)
            }
            LayerOperation::Flatten => {
                if self.layers.len() == 1
                    && self.background.is_none()
                    && self.layers[0].is_raster()
                    && self.layers[0].visible
                    && (self.layers[0].opacity - 1.0).abs() < 1e-4
                    && self.layers[0].blend_mode == BlendMode::Normal
                    && self.layers[0].offset_x == 0
                    && self.layers[0].offset_y == 0
                    && self.layers[0].effect.is_none()
                    && (self.layers[0].mask.is_none() || !self.layers[0].mask_enabled)
                    && self.layers[0].dimensions() == (self.canvas_width, self.canvas_height)
                {
                    return Ok(false);
                }
                let flat = composite_stack(self);
                let id = self.next_id;
                self.next_id += 1;
                self.layers = vec![Layer::new(id, "Background", flat)];
                self.active_layer_id = id;
                self.background = None;
                Ok(true)
            }
            LayerOperation::SetCanvasBackground { color } => {
                if self.background == *color {
                    return Ok(false);
                }
                self.background = *color;
                Ok(true)
            }
            LayerOperation::AddMask { id, reveal_all } => {
                let Some(idx) = self.index_of(*id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                if self.layers[idx].mask.is_some() {
                    return Ok(false);
                }
                let (lw, lh) = self.layers[idx].dimensions();
                let len = (lw as usize) * (lh as usize);
                let data = Arc::new(vec![if *reveal_all { 255u8 } else { 0u8 }; len]);
                self.layers[idx].mask = Some(data);
                self.layers[idx].mask_enabled = true;
                self.layers[idx].editing_mask = true;
                self.layers[idx].mask_linked = true;
                self.layers[idx].mask_offset_x = 0;
                self.layers[idx].mask_offset_y = 0;
                self.active_layer_id = *id;
                Ok(true)
            }
            LayerOperation::DeleteMask { id } => {
                let Some(idx) = self.index_of(*id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                if self.layers[idx].mask.is_none() {
                    return Ok(false);
                }
                self.layers[idx].mask = None;
                self.layers[idx].mask_enabled = true;
                self.layers[idx].editing_mask = false;
                Ok(true)
            }
            LayerOperation::ApplyMask { id } => {
                let Some(idx) = self.index_of(*id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                let Some(mask) = self.layers[idx].mask.take() else {
                    return Ok(false);
                };
                let (lw, lh) = self.layers[idx].dimensions();
                let mox = self.layers[idx].mask_offset_x;
                let moy = self.layers[idx].mask_offset_y;
                if mask.len() == (lw as usize) * (lh as usize) {
                    let mut next = (*self.layers[idx].pixels).clone();
                    for y in 0..lh {
                        let my = y as i32 - moy;
                        for x in 0..lw {
                            let mx = x as i32 - mox;
                            let m = if mx >= 0 && my >= 0 && (mx as u32) < lw && (my as u32) < lh {
                                u16::from(mask[(my as usize) * (lw as usize) + (mx as usize)])
                            } else {
                                0
                            };
                            let px = next.get_pixel_mut(x, y);
                            px.0[3] = ((u16::from(px.0[3]) * m + 127) / 255) as u8;
                        }
                    }
                    self.layers[idx].pixels = Arc::new(next);
                }
                self.layers[idx].mask_enabled = true;
                self.layers[idx].editing_mask = false;
                self.layers[idx].mask_offset_x = 0;
                self.layers[idx].mask_offset_y = 0;
                Ok(true)
            }
            LayerOperation::InvertMask { id } => {
                let Some(idx) = self.index_of(*id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                let Some(mask) = &self.layers[idx].mask else {
                    return Ok(false);
                };
                let inverted: Vec<u8> = mask.iter().map(|&v| 255 - v).collect();
                self.layers[idx].mask = Some(Arc::new(inverted));
                Ok(true)
            }
            LayerOperation::SetMaskEnabled { id, enabled } => {
                let Some(idx) = self.index_of(*id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].mask.is_none() || self.layers[idx].mask_enabled == *enabled {
                    return Ok(false);
                }
                self.layers[idx].mask_enabled = *enabled;
                Ok(true)
            }
            LayerOperation::SetEditingMask { id, editing } => {
                let Some(idx) = self.index_of(*id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                let want = *editing && self.layers[idx].mask.is_some();
                if self.layers[idx].editing_mask == want && self.active_layer_id == *id {
                    return Ok(false);
                }
                self.layers[idx].editing_mask = want;
                self.active_layer_id = *id;
                Ok(true)
            }
            LayerOperation::SetMaskLinked { id, linked } => {
                let Some(idx) = self.index_of(*id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].mask.is_none() || self.layers[idx].mask_linked == *linked {
                    return Ok(false);
                }
                self.layers[idx].mask_linked = *linked;
                Ok(true)
            }
            LayerOperation::AddEffect { name, effect } => {
                let id = self.next_id;
                self.next_id += 1;
                let label = if name.trim().is_empty() {
                    effect.default_name().to_owned()
                } else {
                    name.trim().to_owned()
                };
                let blank = Arc::new(RgbaImage::from_pixel(
                    self.canvas_width.max(1),
                    self.canvas_height.max(1),
                    Rgba([0, 0, 0, 0]),
                ));
                let mut layer = Layer::new(id, label, blank);
                layer.effect = Some(effect.clone());
                let insert_at = (self.active_index() + 1).min(self.layers.len());
                self.layers.insert(insert_at, layer);
                self.active_layer_id = id;
                Ok(true)
            }
            LayerOperation::UpdateEffect { id, effect } => {
                let Some(idx) = self.index_of(*id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                if self.layers[idx].effect.as_ref() == Some(effect) {
                    return Ok(false);
                }
                self.layers[idx].effect = Some(effect.clone());
                Ok(true)
            }
            LayerOperation::AddVectorLayer { name, data } => {
                self.add_vector_layer(name, data.clone());
                Ok(true)
            }
            LayerOperation::UpdateVectorLayer { id, data } => {
                let Some(idx) = self.index_of(*id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                if self.layers[idx].vector_data.as_ref() == Some(data) {
                    return Ok(false);
                }
                self.layers[idx].vector_data = Some(data.clone());
                self.layers[idx].text_data = None;
                let (cw, ch) = (self.canvas_width, self.canvas_height);
                self.layers[idx].refresh_cached_pixels(cw, ch);
                Ok(true)
            }
            LayerOperation::AddVectorShape { layer_id, shape } => {
                let Some(idx) = self.index_of(*layer_id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                let vdata = self.layers[idx]
                    .vector_data
                    .get_or_insert_with(VectorLayerData::new);
                vdata.push_shape(shape.clone());
                let (cw, ch) = (self.canvas_width, self.canvas_height);
                self.layers[idx].refresh_cached_pixels(cw, ch);
                self.active_layer_id = *layer_id;
                Ok(true)
            }
            LayerOperation::UpdateVectorShape { layer_id, shape } => {
                let Some(idx) = self.index_of(*layer_id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                let Some(vdata) = &mut self.layers[idx].vector_data else {
                    return Err(ImageError::coded("not_a_vector_layer"));
                };
                let Some(existing) = vdata.shape_mut(shape.id) else {
                    return Err(ImageError::coded("vector_shape_not_found"));
                };
                if existing == shape {
                    return Ok(false);
                }
                *existing = shape.clone();
                let (cw, ch) = (self.canvas_width, self.canvas_height);
                self.layers[idx].refresh_cached_pixels(cw, ch);
                Ok(true)
            }
            LayerOperation::UpdateVectorPathNodes {
                layer_id,
                shape_id,
                nodes,
                closed,
            } => {
                let Some(idx) = self.index_of(*layer_id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                let Some(vdata) = &mut self.layers[idx].vector_data else {
                    return Err(ImageError::coded("not_a_vector_layer"));
                };
                let Some(shape) = vdata.shape_mut(*shape_id) else {
                    return Err(ImageError::coded("vector_shape_not_found"));
                };
                let next_closed = closed.unwrap_or_else(|| shape.geometry.is_closed());
                let next_geom = photon_protocol::VectorGeometry::Path {
                    nodes: nodes.clone(),
                    closed: next_closed,
                };
                if shape.geometry == next_geom {
                    return Ok(false);
                }
                shape.geometry = next_geom;
                let (cw, ch) = (self.canvas_width, self.canvas_height);
                self.layers[idx].refresh_cached_pixels(cw, ch);
                Ok(true)
            }
            LayerOperation::DeleteVectorShape { layer_id, shape_id } => {
                let Some(idx) = self.index_of(*layer_id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                let Some(vdata) = &mut self.layers[idx].vector_data else {
                    return Err(ImageError::coded("not_a_vector_layer"));
                };
                if vdata.remove_shape(*shape_id).is_none() {
                    return Ok(false);
                }
                let (cw, ch) = (self.canvas_width, self.canvas_height);
                self.layers[idx].refresh_cached_pixels(cw, ch);
                Ok(true)
            }
            LayerOperation::ConvertShapeToPath { layer_id, shape_id } => {
                let Some(idx) = self.index_of(*layer_id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                let Some(vdata) = &mut self.layers[idx].vector_data else {
                    return Err(ImageError::coded("not_a_vector_layer"));
                };
                let Some(shape) = vdata.shape_mut(*shape_id) else {
                    return Err(ImageError::coded("vector_shape_not_found"));
                };
                let changed = convert_shape_to_path(shape);
                if changed {
                    let (cw, ch) = (self.canvas_width, self.canvas_height);
                    self.layers[idx].refresh_cached_pixels(cw, ch);
                }
                Ok(changed)
            }
            LayerOperation::VectorPathOp {
                layer_id,
                shape_id,
                op,
            } => {
                let Some(idx) = self.index_of(*layer_id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                let Some(vdata) = &mut self.layers[idx].vector_data else {
                    return Err(ImageError::coded("not_a_vector_layer"));
                };
                if let VectorPathOperation::SelectShapes { shape_ids } = op {
                    let valid: Vec<u64> = shape_ids
                        .iter()
                        .copied()
                        .filter(|&sid| vdata.shape(sid).is_some())
                        .collect();
                    if vdata.selected_shape_ids == valid {
                        return Ok(false);
                    }
                    vdata.selected_shape_ids = valid;
                    return Ok(true);
                }
                let Some(sid) = shape_id
                    .or_else(|| vdata.selected_shape_ids.last().copied())
                    .or_else(|| vdata.shapes.last().map(|s| s.id))
                else {
                    return Err(ImageError::coded("vector_shape_not_found"));
                };
                let changed = match op {
                    VectorPathOperation::AddNodeOnSegment { segment_index, t } => {
                        let Some(shape) = vdata.shape_mut(sid) else {
                            return Err(ImageError::coded("vector_shape_not_found"));
                        };
                        let (mut nodes, closed) = geometry_to_bezier_path(&shape.geometry);
                        if add_node_on_segment(&mut nodes, closed, *segment_index, *t).is_some() {
                            shape.geometry = VectorGeometry::Path { nodes, closed };
                            true
                        } else {
                            false
                        }
                    }
                    VectorPathOperation::AddNodeAtPoint {
                        point,
                        max_distance,
                    } => {
                        let Some(shape) = vdata.shape_mut(sid) else {
                            return Err(ImageError::coded("vector_shape_not_found"));
                        };
                        let (mut nodes, closed) = geometry_to_bezier_path(&shape.geometry);
                        if add_node_at_closest_point(&mut nodes, closed, *point, *max_distance)
                            .is_some()
                        {
                            shape.geometry = VectorGeometry::Path { nodes, closed };
                            true
                        } else {
                            false
                        }
                    }
                    VectorPathOperation::DeleteNode { index } => {
                        let Some(shape) = vdata.shape_mut(sid) else {
                            return Err(ImageError::coded("vector_shape_not_found"));
                        };
                        let (mut nodes, closed) = geometry_to_bezier_path(&shape.geometry);
                        if nodes.len() <= 2 || delete_node(&mut nodes, *index).is_none() {
                            false
                        } else {
                            shape.geometry = VectorGeometry::Path { nodes, closed };
                            true
                        }
                    }
                    VectorPathOperation::MoveNodeAnchor { index, anchor } => {
                        let Some(shape) = vdata.shape_mut(sid) else {
                            return Err(ImageError::coded("vector_shape_not_found"));
                        };
                        let (mut nodes, closed) = geometry_to_bezier_path(&shape.geometry);
                        if move_node_anchor(&mut nodes, *index, *anchor) {
                            shape.geometry = VectorGeometry::Path { nodes, closed };
                            true
                        } else {
                            false
                        }
                    }
                    VectorPathOperation::MoveNodeHandle {
                        index,
                        handle,
                        position,
                        mode,
                    } => {
                        let Some(shape) = vdata.shape_mut(sid) else {
                            return Err(ImageError::coded("vector_shape_not_found"));
                        };
                        let (mut nodes, closed) = geometry_to_bezier_path(&shape.geometry);
                        if move_node_handle(&mut nodes, *index, *handle, *position, *mode) {
                            shape.geometry = VectorGeometry::Path { nodes, closed };
                            true
                        } else {
                            false
                        }
                    }
                    VectorPathOperation::SplitAtNode { index } => {
                        let Some(pos) = vdata.index_of(sid) else {
                            return Err(ImageError::coded("vector_shape_not_found"));
                        };
                        let mut next_id = vdata
                            .next_shape_id
                            .max(vdata.shapes.iter().map(|s| s.id).max().unwrap_or(0) + 1);
                        let parts = split_shape_at_node(&vdata.shapes[pos], *index, &mut next_id);
                        vdata.next_shape_id = next_id;
                        if parts.len() == 1 && parts[0] == vdata.shapes[pos] {
                            false
                        } else {
                            let new_ids: Vec<u64> = parts.iter().map(|s| s.id).collect();
                            vdata.shapes.splice(pos..=pos, parts);
                            vdata.selected_shape_ids = new_ids;
                            true
                        }
                    }
                    VectorPathOperation::JoinPaths {
                        second_shape_id,
                        tolerance,
                    } => {
                        let Some(second_id) = second_shape_id
                            .or_else(|| {
                                vdata
                                    .selected_shape_ids
                                    .iter()
                                    .copied()
                                    .find(|&id| id != sid)
                            })
                            .or_else(|| {
                                vdata
                                    .shapes
                                    .iter()
                                    .rev()
                                    .map(|s| s.id)
                                    .find(|&id| id != sid)
                            })
                        else {
                            return Ok(false);
                        };
                        if second_id == sid {
                            return Ok(false);
                        }
                        let Some(shape_a) = vdata.shape(sid).cloned() else {
                            return Err(ImageError::coded("vector_shape_not_found"));
                        };
                        let Some(shape_b) = vdata.shape(second_id).cloned() else {
                            return Err(ImageError::coded("vector_shape_not_found"));
                        };
                        let joined = join_shapes(&shape_a, &shape_b, *tolerance);
                        if let Some(target) = vdata.shape_mut(sid) {
                            *target = joined;
                        }
                        vdata.remove_shape(second_id);
                        vdata.selected_shape_ids = vec![sid];
                        true
                    }
                    VectorPathOperation::SetClosed { closed } => {
                        let Some(shape) = vdata.shape_mut(sid) else {
                            return Err(ImageError::coded("vector_shape_not_found"));
                        };
                        set_path_closed(&mut shape.geometry, *closed)
                    }
                    VectorPathOperation::ToggleClosed => {
                        let Some(shape) = vdata.shape_mut(sid) else {
                            return Err(ImageError::coded("vector_shape_not_found"));
                        };
                        toggle_path_closed(&mut shape.geometry);
                        true
                    }
                    VectorPathOperation::ReversePath => {
                        let Some(shape) = vdata.shape_mut(sid) else {
                            return Err(ImageError::coded("vector_shape_not_found"));
                        };
                        reverse_shape_path(shape)
                    }
                    VectorPathOperation::SetNodeMode { index, mode } => {
                        let Some(shape) = vdata.shape_mut(sid) else {
                            return Err(ImageError::coded("vector_shape_not_found"));
                        };
                        let (mut nodes, closed) = geometry_to_bezier_path(&shape.geometry);
                        if set_node_mode(&mut nodes, closed, *index, *mode) {
                            shape.geometry = VectorGeometry::Path { nodes, closed };
                            true
                        } else {
                            false
                        }
                    }
                    VectorPathOperation::Simplify { tolerance } => {
                        let Some(shape) = vdata.shape_mut(sid) else {
                            return Err(ImageError::coded("vector_shape_not_found"));
                        };
                        simplify_shape(shape, *tolerance)
                    }
                    VectorPathOperation::Smooth { tension } => {
                        let Some(shape) = vdata.shape_mut(sid) else {
                            return Err(ImageError::coded("vector_shape_not_found"));
                        };
                        smooth_shape(shape, *tension)
                    }
                    VectorPathOperation::TranslateShape { dx, dy } => {
                        if dx.abs() < 1e-4 && dy.abs() < 1e-4 {
                            false
                        } else {
                            let Some(shape) = vdata.shape_mut(sid) else {
                                return Err(ImageError::coded("vector_shape_not_found"));
                            };
                            let step_spec = TransformEachSpec {
                                translate_x: *dx,
                                translate_y: *dy,
                                ..TransformEachSpec::default()
                            };
                            *shape = transform_shape_once(shape, &step_spec);
                            true
                        }
                    }
                    VectorPathOperation::SelectShapes { .. } => false,
                };
                if changed {
                    let (cw, ch) = (self.canvas_width, self.canvas_height);
                    self.layers[idx].refresh_cached_pixels(cw, ch);
                }
                Ok(changed)
            }
            LayerOperation::TransformEach {
                layer_ids,
                shape_ids,
                spec,
            } => {
                if spec.is_identity() {
                    return Ok(false);
                }
                let (cw, ch) = (self.canvas_width, self.canvas_height);
                // Vector-shape mode when `shape_ids` are provided or the single target/active layer is a Vector layer.
                let single_target_id = match layer_ids.as_slice() {
                    [] => Some(self.active_layer_id),
                    [only] => Some(*only),
                    _ => None,
                };
                if let Some(tid) = single_target_id
                    && let Some(idx) = self.index_of(tid)
                    && self.layers[idx].is_vector()
                {
                    if self.layers[idx].locked {
                        return Err(ImageError::coded("layer_locked"));
                    }
                    if let Some(vdata) = &mut self.layers[idx].vector_data {
                        if vdata.is_empty() {
                            return Ok(false);
                        }
                        let targets = (!shape_ids.is_empty()).then_some(shape_ids.as_slice());
                        apply_transform_each_to_layer(vdata, targets, spec);
                    }
                    self.layers[idx].refresh_cached_pixels(cw, ch);
                    return Ok(true);
                }

                // Multi-layer / Raster & Text layer Transform Each mode.
                let targets: Vec<u64> = if !layer_ids.is_empty() {
                    layer_ids.clone()
                } else {
                    let unlocked: Vec<u64> = self
                        .layers
                        .iter()
                        .filter(|l| !l.locked && l.visible)
                        .map(|l| l.id)
                        .collect();
                    if unlocked.is_empty() {
                        vec![self.active_layer_id]
                    } else {
                        unlocked
                    }
                };
                let steps = spec.repeat_steps.max(1);
                let mut any_changed = false;
                for tid in targets {
                    let Some(idx) = self.index_of(tid) else {
                        continue;
                    };
                    if self.layers[idx].locked {
                        continue;
                    }
                    if spec.apply_to_duplicate {
                        let base_layer = self.layers[idx].clone();
                        let mut cur = base_layer.clone();
                        let mut insert_pos = idx + 1;
                        for step in 1..=steps {
                            cur = transform_single_layer_once(&cur, spec, cw, ch);
                            let new_id = self.next_id;
                            self.next_id += 1;
                            let mut dup = cur.clone();
                            dup.id = new_id;
                            dup.name = if steps == 1 {
                                format!("{} copy", base_layer.name)
                            } else {
                                format!("{} copy {step}", base_layer.name)
                            };
                            self.layers.insert(insert_pos, dup);
                            insert_pos += 1;
                            self.active_layer_id = new_id;
                            any_changed = true;
                        }
                    } else {
                        let mut cur = self.layers[idx].clone();
                        for _ in 0..steps {
                            cur = transform_single_layer_once(&cur, spec, cw, ch);
                        }
                        self.layers[idx] = cur;
                        any_changed = true;
                    }
                }
                Ok(any_changed)
            }
            LayerOperation::VectorBlend { layer_id, spec } => {
                let Some(idx) = self.index_of(*layer_id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                let Some(vdata) = &mut self.layers[idx].vector_data else {
                    return Err(ImageError::coded("not_a_vector_layer"));
                };
                let created = apply_blend_to_layer(vdata, spec)?;
                if created.is_empty() {
                    return Ok(false);
                }
                vdata.selected_shape_ids = created;
                let (cw, ch) = (self.canvas_width, self.canvas_height);
                self.layers[idx].refresh_cached_pixels(cw, ch);
                Ok(true)
            }
            LayerOperation::AddTextLayer { name, data } => {
                self.add_text_layer(name, data.clone());
                Ok(true)
            }
            LayerOperation::UpdateTextLayer { id, data } => {
                let Some(idx) = self.index_of(*id) else {
                    return Err(ImageError::coded("layer_not_found"));
                };
                if self.layers[idx].locked {
                    return Err(ImageError::coded("layer_locked"));
                }
                if self.layers[idx].text_data.as_ref() == Some(data) {
                    return Ok(false);
                }
                self.layers[idx].text_data = Some(data.clone());
                self.layers[idx].vector_data = None;
                let (cw, ch) = (self.canvas_width, self.canvas_height);
                self.layers[idx].refresh_cached_pixels(cw, ch);
                Ok(true)
            }
            LayerOperation::RasterizeLayer { id } => self.rasterize_layer(*id),
        }
    }
}

fn anchor_pivot_uv(anchor: CanvasAnchor) -> (f32, f32) {
    let px = match anchor {
        CanvasAnchor::TopLeft | CanvasAnchor::Left | CanvasAnchor::BottomLeft => 0.0,
        CanvasAnchor::Top | CanvasAnchor::Center | CanvasAnchor::Bottom => 0.5,
        CanvasAnchor::TopRight | CanvasAnchor::Right | CanvasAnchor::BottomRight => 1.0,
    };
    let py = match anchor {
        CanvasAnchor::TopLeft | CanvasAnchor::Top | CanvasAnchor::TopRight => 0.0,
        CanvasAnchor::Left | CanvasAnchor::Center | CanvasAnchor::Right => 0.5,
        CanvasAnchor::BottomLeft | CanvasAnchor::Bottom | CanvasAnchor::BottomRight => 1.0,
    };
    (px, py)
}

fn transform_single_layer_once(
    layer: &Layer,
    spec: &TransformEachSpec,
    canvas_w: u32,
    canvas_h: u32,
) -> Layer {
    let mut out = layer.clone();
    let one_step = TransformEachSpec {
        repeat_steps: 1,
        apply_to_duplicate: false,
        ..*spec
    };
    if let Some(ref mut vdata) = out.vector_data {
        apply_transform_each_to_layer(vdata, None, &one_step);
        out.refresh_cached_pixels(canvas_w, canvas_h);
        return out;
    }
    if let Some(ref mut tdata) = out.text_data {
        let scale = ((spec.scale_x.abs() + spec.scale_y.abs()) * 0.5).max(0.1);
        tdata.font_size = (tdata.font_size * scale).clamp(4.0, 512.0);
        tdata.x += spec.translate_x;
        tdata.y += spec.translate_y;
        out.refresh_cached_pixels(canvas_w, canvas_h);
        return out;
    }

    let (pivot_x, pivot_y) = anchor_pivot_uv(spec.anchor);
    let lt = LayerTransformSpec {
        translate_x: spec.translate_x,
        translate_y: spec.translate_y,
        scale_x: spec.scale_x,
        scale_y: spec.scale_y,
        rotation_deg: spec.rotation_deg,
        flip_h: spec.reflect_x,
        flip_v: spec.reflect_y,
        pivot_x,
        pivot_y,
    };
    let (lw, lh) = out.dimensions();
    let (next_px, dx, dy) = crate::pixels::transform_rgba_affine(&out.pixels, &lt);
    out.pixels = Arc::new(next_px);
    out.offset_x += dx;
    out.offset_y += dy;
    if let Some(orig) = &out.original {
        let (next_orig, _, _) = crate::pixels::transform_rgba_affine(orig, &lt);
        out.original = Some(Arc::new(next_orig));
    }
    if let Some(mask) = &out.mask
        && mask.len() == (lw as usize) * (lh as usize)
    {
        if out.mask_linked {
            let (next_mask, _, _, _, _) = crate::pixels::transform_mask_affine(mask, lw, lh, &lt);
            out.mask = Some(Arc::new(next_mask));
        } else {
            out.mask_offset_x -= dx;
            out.mask_offset_y -= dy;
        }
    }
    out
}

/// Composite all visible layers in `stack` onto a `canvas_width x canvas_height` RGBA buffer.
///
/// Uses an O(1) zero-copy fast path for unmodified single-layer normal images without mask or backdrop.
pub fn composite_stack(stack: &LayerStack) -> Arc<RgbaImage> {
    if stack.layers.len() == 1 && stack.background.is_none() {
        let l = &stack.layers[0];
        if l.visible
            && (l.opacity - 1.0).abs() < 1e-4
            && l.blend_mode == BlendMode::Normal
            && l.offset_x == 0
            && l.offset_y == 0
            && l.effect.is_none()
            && (l.mask.is_none() || !l.mask_enabled)
            && l.dimensions() == (stack.canvas_width, stack.canvas_height)
        {
            return Arc::clone(&l.pixels);
        }
    }
    let base_px = match stack.background {
        Some([r, g, b]) => Rgba([r, g, b, 255]),
        None => Rgba([0, 0, 0, 0]),
    };
    let mut canvas = RgbaImage::from_pixel(
        stack.canvas_width.max(1),
        stack.canvas_height.max(1),
        base_px,
    );
    for layer in &stack.layers {
        if !layer.visible || layer.opacity <= 1e-4 {
            continue;
        }
        blend_layer_onto(&mut canvas, layer);
    }
    Arc::new(canvas)
}

/// Composite all visible layers in `stack` except `exclude_layer_id` (used for live GPU Transform preview).
pub fn composite_stack_excluding(stack: &LayerStack, exclude_layer_id: u64) -> Arc<RgbaImage> {
    let base_px = match stack.background {
        Some([r, g, b]) => Rgba([r, g, b, 255]),
        None => Rgba([0, 0, 0, 0]),
    };
    let mut canvas = RgbaImage::from_pixel(
        stack.canvas_width.max(1),
        stack.canvas_height.max(1),
        base_px,
    );
    for layer in &stack.layers {
        if layer.id == exclude_layer_id || !layer.visible || layer.opacity <= 1e-4 {
            continue;
        }
        blend_layer_onto(&mut canvas, layer);
    }
    Arc::new(canvas)
}

fn composite_pair(canvas_w: u32, canvas_h: u32, bottom: &Layer, top: &Layer) -> RgbaImage {
    let mut canvas = RgbaImage::from_pixel(canvas_w.max(1), canvas_h.max(1), Rgba([0, 0, 0, 0]));
    if bottom.visible && bottom.opacity > 1e-4 {
        blend_layer_onto(&mut canvas, bottom);
    }
    if top.visible && top.opacity > 1e-4 {
        blend_layer_onto(&mut canvas, top);
    }
    canvas
}

#[inline]
fn blend_channel(mode: BlendMode, dst: f32, src: f32) -> f32 {
    match mode {
        BlendMode::Normal => src,
        BlendMode::Multiply => dst * src,
        BlendMode::Screen => 1.0 - (1.0 - dst) * (1.0 - src),
        BlendMode::Overlay => {
            if dst < 0.5 {
                2.0 * dst * src
            } else {
                1.0 - 2.0 * (1.0 - dst) * (1.0 - src)
            }
        }
        BlendMode::Darken => dst.min(src),
        BlendMode::Lighten => dst.max(src),
        BlendMode::Difference => (dst - src).abs(),
    }
}

fn apply_layer_effect(canvas: &RgbaImage, effect: &LayerEffect) -> RgbaImage {
    match effect {
        LayerEffect::ColorLight(adj) => crate::pixels::adjust_color_and_light(canvas, adj),
        LayerEffect::Hsv(adj) => crate::pixels::adjust_hsv(canvas, adj),
        LayerEffect::ColorBalance(adj) => crate::pixels::color_balance(canvas, adj),
        LayerEffect::Curves(curves) => crate::pixels::adjust_curves(canvas, curves),
        LayerEffect::Blur { radius } => crate::pixels::box_blur_rgba(canvas, *radius),
        LayerEffect::Sharpen { amount } => crate::pixels::sharpen_rgba(canvas, *amount),
        LayerEffect::Denoise {
            strength,
            detail_preserve,
        } => crate::pixels::denoise_rgba(canvas, *strength, *detail_preserve, None),
        LayerEffect::Invert => crate::pixels::invert_rgba(canvas),
        LayerEffect::Desaturate => crate::pixels::desaturate_rgba(canvas),
        LayerEffect::Sepia => crate::pixels::sepia_rgba(canvas),
    }
}

/// Blend `layer` onto `canvas` using W3C Compositing & Blending Level 1 math.
pub fn blend_layer_onto(canvas: &mut RgbaImage, layer: &Layer) {
    let (cw, ch) = canvas.dimensions();
    if let Some(effect) = &layer.effect {
        let adjusted = apply_layer_effect(canvas, effect);
        let opacity = layer.opacity.clamp(0.0, 1.0);
        let (lw, lh) = layer.dimensions();
        let active_mask = if layer.mask_enabled {
            layer
                .mask
                .as_deref()
                .filter(|m| m.len() == (lw as usize) * (lh as usize))
        } else {
            None
        };
        let mox = layer.mask_offset_x;
        let moy = layer.mask_offset_y;
        let mode = layer.blend_mode;
        for y in 0..ch {
            let my = y as i32 - moy;
            for x in 0..cw {
                let m = match active_mask {
                    Some(mask) => {
                        let mx = x as i32 - mox;
                        if mx >= 0 && my >= 0 && (mx as u32) < lw && (my as u32) < lh {
                            mask[(my as usize) * (lw as usize) + (mx as usize)] as f32 / 255.0
                        } else {
                            0.0
                        }
                    }
                    None => 1.0,
                };
                let t = (opacity * m).clamp(0.0, 1.0);
                if t <= 1e-5 {
                    continue;
                }
                let dp = canvas.get_pixel(x, y).0;
                let sp = adjusted.get_pixel(x, y).0;
                let mix = |d: u8, s: u8| -> u8 {
                    let df = f32::from(d) / 255.0;
                    let sf = f32::from(s) / 255.0;
                    let b = blend_channel(mode, df, sf);
                    ((df * (1.0 - t) + b * t) * 255.0).round().clamp(0.0, 255.0) as u8
                };
                canvas.put_pixel(
                    x,
                    y,
                    Rgba([
                        mix(dp[0], sp[0]),
                        mix(dp[1], sp[1]),
                        mix(dp[2], sp[2]),
                        dp[3],
                    ]),
                );
            }
        }
        return;
    }

    let (lw, lh) = layer.pixels.dimensions();
    let ox = layer.offset_x;
    let oy = layer.offset_y;

    let x_start = ox.max(0);
    let y_start = oy.max(0);
    let x_end = (ox + lw as i32).min(cw as i32);
    let y_end = (oy + lh as i32).min(ch as i32);
    if x_start >= x_end || y_start >= y_end {
        return;
    }

    let opacity = layer.opacity.clamp(0.0, 1.0);
    let mode = layer.blend_mode;
    let active_mask = if layer.mask_enabled {
        layer
            .mask
            .as_deref()
            .filter(|m| m.len() == (lw as usize) * (lh as usize))
    } else {
        None
    };
    let mox = layer.mask_offset_x;
    let moy = layer.mask_offset_y;

    for cy in y_start..y_end {
        let ly = (cy - oy) as u32;
        let my = ly as i32 - moy;
        for cx in x_start..x_end {
            let lx = (cx - ox) as u32;
            let sp = layer.pixels.get_pixel(lx, ly);
            let mask_factor = match active_mask {
                Some(m) => {
                    let mx = lx as i32 - mox;
                    if mx >= 0 && my >= 0 && (mx as u32) < lw && (my as u32) < lh {
                        m[(my as usize) * (lw as usize) + (mx as usize)] as f32 / 255.0
                    } else {
                        0.0
                    }
                }
                None => 1.0,
            };
            let sa = (sp[3] as f32 / 255.0) * opacity * mask_factor;
            if sa <= 1e-5 {
                continue;
            }
            let dp = canvas.get_pixel(cx as u32, cy as u32);
            let da = dp[3] as f32 / 255.0;

            if da <= 1e-5 && mode == BlendMode::Normal {
                let out_a = (sa * 255.0).round().clamp(0.0, 255.0) as u8;
                canvas.put_pixel(cx as u32, cy as u32, Rgba([sp[0], sp[1], sp[2], out_a]));
                continue;
            }

            let sr = sp[0] as f32 / 255.0;
            let sg = sp[1] as f32 / 255.0;
            let sb = sp[2] as f32 / 255.0;
            let dr = dp[0] as f32 / 255.0;
            let dg = dp[1] as f32 / 255.0;
            let db = dp[2] as f32 / 255.0;

            let br = blend_channel(mode, dr, sr);
            let bg = blend_channel(mode, dg, sg);
            let bb = blend_channel(mode, db, sb);

            let out_a = sa + da * (1.0 - sa);
            if out_a <= 1e-5 {
                canvas.put_pixel(cx as u32, cy as u32, Rgba([0, 0, 0, 0]));
                continue;
            }
            let r_eff = (1.0 - da) * sr + da * br;
            let g_eff = (1.0 - da) * sg + da * bg;
            let b_eff = (1.0 - da) * sb + da * bb;

            let out_r = ((sa * r_eff + (1.0 - sa) * da * dr) / out_a * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            let out_g = ((sa * g_eff + (1.0 - sa) * da * dg) / out_a * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            let out_b = ((sa * b_eff + (1.0 - sa) * da * db) / out_a * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            let out_au = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;

            canvas.put_pixel(cx as u32, cy as u32, Rgba([out_r, out_g, out_b, out_au]));
        }
    }
}

/// Compact RGBA thumbnails for each layer and optional grayscale mask.
#[derive(Debug, Clone)]
pub struct LayerThumbnail {
    pub layer_id: u64,
    pub width: u32,
    pub height: u32,
    pub layer_rgba: Vec<u8>,
    pub mask_rgba: Option<Vec<u8>>,
}

/// Generate compact aspect-preserving thumbnails (`max_edge` px) for all layers and masks.
pub fn render_layer_thumbnails(stack: &LayerStack, max_edge: u32) -> Vec<LayerThumbnail> {
    let cap = max_edge.max(8);
    stack
        .layers
        .iter()
        .map(|layer| {
            let (lw, lh) = layer.dimensions();
            let lw = lw.max(1);
            let lh = lh.max(1);
            let scale = (cap as f32 / lw.max(lh) as f32).min(1.0);
            let tw = ((lw as f32 * scale).round() as u32).max(1);
            let th = ((lh as f32 * scale).round() as u32).max(1);
            let mut layer_rgba = vec![0u8; (tw as usize) * (th as usize) * 4];
            let mut mask_rgba = layer
                .mask
                .as_ref()
                .map(|_| vec![255u8; (tw as usize) * (th as usize) * 4]);
            let mask_slice = layer
                .mask
                .as_deref()
                .filter(|m| m.len() == (lw as usize) * (lh as usize));

            for ty in 0..th {
                let sy = ((ty as u64 * lh as u64) / th as u64).min((lh - 1) as u64) as u32;
                let m_row = (sy as usize) * (lw as usize);
                for tx in 0..tw {
                    let sx = ((tx as u64 * lw as u64) / tw as u64).min((lw - 1) as u64) as u32;
                    let dst_idx = ((ty as usize) * (tw as usize) + (tx as usize)) * 4;
                    if let Some(effect) = &layer.effect {
                        let [c0, c1] = if effect.is_filter() {
                            [[56u8, 108, 168], [82u8, 146, 212]]
                        } else {
                            [[142u8, 96, 178], [204u8, 146, 86]]
                        };
                        let diag = ((tx + ty) * 255) / (tw + th).max(1);
                        let t = diag;
                        let blend_c = |a: u8, b: u8| -> u8 {
                            ((u32::from(a) * (255 - t) + u32::from(b) * t) / 255) as u8
                        };
                        layer_rgba[dst_idx] = blend_c(c0[0], c1[0]);
                        layer_rgba[dst_idx + 1] = blend_c(c0[1], c1[1]);
                        layer_rgba[dst_idx + 2] = blend_c(c0[2], c1[2]);
                        layer_rgba[dst_idx + 3] = 255;
                    } else {
                        let p = layer.pixels.get_pixel(sx, sy).0;
                        let checker = if ((tx / 4) + (ty / 4)).is_multiple_of(2) {
                            62u16
                        } else {
                            92u16
                        };
                        let a = u16::from(p[3]);
                        let inv_a = 255 - a;
                        layer_rgba[dst_idx] = ((u16::from(p[0]) * a + checker * inv_a) / 255) as u8;
                        layer_rgba[dst_idx + 1] =
                            ((u16::from(p[1]) * a + checker * inv_a) / 255) as u8;
                        layer_rgba[dst_idx + 2] =
                            ((u16::from(p[2]) * a + checker * inv_a) / 255) as u8;
                        layer_rgba[dst_idx + 3] = 255;
                    }
                    if let (Some(out_m), Some(src_m)) = (mask_rgba.as_mut(), mask_slice) {
                        let v = src_m[m_row + sx as usize];
                        out_m[dst_idx] = v;
                        out_m[dst_idx + 1] = v;
                        out_m[dst_idx + 2] = v;
                        out_m[dst_idx + 3] = 255;
                    }
                }
            }

            LayerThumbnail {
                layer_id: layer.id,
                width: tw,
                height: th,
                layer_rgba,
                mask_rgba,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use photon_protocol::{TextAlign, VectorStroke};

    use super::*;

    #[test]
    fn vector_and_text_layers_report_kind_and_rasterize_cleanly() {
        let base = Arc::new(RgbaImage::from_pixel(64, 64, Rgba([20, 20, 20, 255])));
        let mut stack = LayerStack::from_single(base);
        assert_eq!(stack.summaries()[0].kind, LayerKind::Raster);

        let rect = VectorShape::new_rect(
            1,
            "Red Square",
            8.0,
            8.0,
            24.0,
            24.0,
            0.0,
            Some([250, 30, 30]),
            VectorStroke::none(),
        );
        stack
            .apply_op(&LayerOperation::AddVectorLayer {
                name: "Shapes".to_owned(),
                data: VectorLayerData::from_shapes(vec![rect]),
            })
            .expect("add vector layer");
        let vector_id = stack.active_layer_id;
        assert_eq!(stack.active_layer().kind(), LayerKind::Vector);
        assert_eq!(stack.summaries()[1].kind, LayerKind::Vector);

        stack
            .apply_op(&LayerOperation::AddTextLayer {
                name: String::new(),
                data: TextLayerData {
                    text: "TITLE".to_owned(),
                    font_size: 20.0,
                    color: [40, 220, 250],
                    align: TextAlign::Left,
                    x: 4.0,
                    y: 36.0,
                    ..TextLayerData::default()
                },
            })
            .expect("add text layer");
        let text_id = stack.active_layer_id;
        assert_eq!(stack.active_layer().kind(), LayerKind::Text);
        assert_eq!(stack.active_layer().name, "TITLE");
        assert_eq!(stack.summaries()[2].kind, LayerKind::Text);

        // Compositing and thumbnail generation work seamlessly across Raster + Vector + Text.
        let composited = composite_stack(&stack);
        assert_eq!(composited.get_pixel(16, 16).0, [250, 30, 30, 255]);
        let thumbs = render_layer_thumbnails(&stack, 32);
        assert_eq!(thumbs.len(), 3);

        // Rasterize both vector and text layers -> kind becomes Raster while pixels stay intact.
        assert!(
            stack
                .apply_op(&LayerOperation::RasterizeLayer { id: vector_id })
                .expect("rasterize vector")
        );
        assert!(
            stack
                .apply_op(&LayerOperation::RasterizeLayer { id: text_id })
                .expect("rasterize text")
        );
        for summary in stack.summaries() {
            assert_eq!(summary.kind, LayerKind::Raster);
        }
        assert_eq!(
            composite_stack(&stack).get_pixel(16, 16).0,
            [250, 30, 30, 255]
        );
    }

    #[test]
    fn vector_layer_operations_add_update_convert_and_delete_shapes() {
        let base = Arc::new(RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 255])));
        let mut stack = LayerStack::from_single(base);
        let layer_id = stack.add_vector_layer("Vectors", VectorLayerData::new());

        let shape = VectorShape::new_rect(
            1,
            "Box",
            10.0,
            10.0,
            20.0,
            20.0,
            0.0,
            Some([200, 100, 50]),
            VectorStroke::none(),
        );
        assert!(
            stack
                .apply_op(&LayerOperation::AddVectorShape {
                    layer_id,
                    shape: shape.clone(),
                })
                .expect("add shape")
        );
        assert!(
            stack
                .apply_op(&LayerOperation::ConvertShapeToPath {
                    layer_id,
                    shape_id: 1,
                })
                .expect("convert shape to path")
        );
        assert!(
            stack
                .apply_op(&LayerOperation::DeleteVectorShape {
                    layer_id,
                    shape_id: 1,
                })
                .expect("delete shape")
        );
        assert!(
            stack
                .active_layer()
                .vector_data
                .as_ref()
                .expect("vector data")
                .is_empty()
        );
    }
}
