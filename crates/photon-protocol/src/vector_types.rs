//! Protocol types for non-destructive Vector Layers, vector shapes, path node editing,
//! `Transform Each`, and `Vector Blend`.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{CanvasAnchor, VectorPathNode, default_one_f32, default_true};

fn default_stroke_width() -> f32 {
    2.0
}

fn default_next_shape_id() -> u64 {
    1
}

fn default_repeat_steps() -> u32 {
    1
}

fn default_blend_steps() -> u32 {
    5
}

fn default_stroke_color() -> [u8; 3] {
    [255, 255, 255]
}

fn deserialize_vector_stroke<'de, D>(deserializer: D) -> Result<VectorStroke, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<VectorStroke>::deserialize(deserializer)?;
    Ok(opt.unwrap_or_else(VectorStroke::none))
}

/// Discriminator for a layer's primary content model (`Raster`, `Vector`, or `Text`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    #[default]
    Raster,
    Vector,
    Text,
}

impl LayerKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Raster => "raster",
            Self::Vector => "vector",
            Self::Text => "text",
        }
    }

    /// Short badge label for the Layers panel (`Some("V")` for Vector, `Some("T")` for Text, `None` for Raster).
    pub const fn badge(self) -> Option<&'static str> {
        match self {
            Self::Raster => None,
            Self::Vector => Some("V"),
            Self::Text => Some("T"),
        }
    }

    pub const fn is_raster(self) -> bool {
        matches!(self, Self::Raster)
    }

    pub const fn is_vector(self) -> bool {
        matches!(self, Self::Vector)
    }

    pub const fn is_text(self) -> bool {
        matches!(self, Self::Text)
    }
}

/// Tangent handle constraint mode for a [`VectorPathNode`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VectorNodeMode {
    /// Handles move independently (or are absent for a sharp corner vertex).
    #[default]
    Corner,
    /// In/out handles stay collinear (180 degrees apart) with independent lengths.
    Smooth,
    /// In/out handles stay collinear (180 degrees apart) with equal lengths.
    Symmetric,
}

impl VectorNodeMode {
    pub const ALL: [Self; 3] = [Self::Corner, Self::Smooth, Self::Symmetric];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Corner => "corner",
            Self::Smooth => "smooth",
            Self::Symmetric => "symmetric",
        }
    }
}

/// Which control handle of a [`VectorPathNode`] is being manipulated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VectorHandleKind {
    HandleIn,
    HandleOut,
}

impl VectorPathNode {
    /// Construct a sharp corner node with no control handles.
    pub const fn corner(anchor: [f32; 2]) -> Self {
        Self {
            anchor,
            handle_in: None,
            handle_out: None,
        }
    }

    /// Construct a node with explicit optional control handles.
    pub const fn new(
        anchor: [f32; 2],
        handle_in: Option<[f32; 2]>,
        handle_out: Option<[f32; 2]>,
    ) -> Self {
        Self {
            anchor,
            handle_in,
            handle_out,
        }
    }

    /// Infer the [`VectorNodeMode`] from the current `handle_in` and `handle_out` positions.
    pub fn inferred_mode(&self) -> VectorNodeMode {
        let (Some(h_in), Some(h_out)) = (self.handle_in, self.handle_out) else {
            return VectorNodeMode::Corner;
        };
        let vin = [h_in[0] - self.anchor[0], h_in[1] - self.anchor[1]];
        let vout = [h_out[0] - self.anchor[0], h_out[1] - self.anchor[1]];
        let len_in = vin[0].hypot(vin[1]);
        let len_out = vout[0].hypot(vout[1]);
        if len_in <= 1e-4 || len_out <= 1e-4 {
            return VectorNodeMode::Corner;
        }
        // Collinear in opposite directions means normalized dot product is close to -1.0.
        let dot = (vin[0] * vout[0] + vin[1] * vout[1]) / (len_in * len_out);
        if dot > -0.995 {
            return VectorNodeMode::Corner;
        }
        if (len_in - len_out).abs() <= 0.05 * len_in.max(len_out).max(1.0) {
            VectorNodeMode::Symmetric
        } else {
            VectorNodeMode::Smooth
        }
    }
}

/// Line cap style for vector strokes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VectorStrokeCap {
    #[default]
    Round,
    Butt,
    Square,
}

pub type StrokeCap = VectorStrokeCap;

impl VectorStrokeCap {
    pub const ALL: [Self; 3] = [Self::Round, Self::Butt, Self::Square];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Round => "round",
            Self::Butt => "butt",
            Self::Square => "square",
        }
    }
}

/// Line join style for vector strokes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VectorStrokeJoin {
    #[default]
    Round,
    Miter,
    Bevel,
}

pub type StrokeJoin = VectorStrokeJoin;

impl VectorStrokeJoin {
    pub const ALL: [Self; 3] = [Self::Round, Self::Miter, Self::Bevel];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Round => "round",
            Self::Miter => "miter",
            Self::Bevel => "bevel",
        }
    }
}

/// Stroke styling for a [`VectorShape`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VectorStroke {
    #[serde(default = "default_stroke_color")]
    pub color: [u8; 3],
    #[serde(default = "default_stroke_width")]
    pub width: f32,
    #[serde(default)]
    pub cap: VectorStrokeCap,
    #[serde(default)]
    pub join: VectorStrokeJoin,
}

impl Default for VectorStroke {
    fn default() -> Self {
        Self {
            color: default_stroke_color(),
            width: default_stroke_width(),
            cap: VectorStrokeCap::Round,
            join: VectorStrokeJoin::Round,
        }
    }
}

impl VectorStroke {
    pub const fn new(color: [u8; 3], width: f32) -> Self {
        Self {
            color,
            width,
            cap: VectorStrokeCap::Round,
            join: VectorStrokeJoin::Round,
        }
    }

    pub const fn with_cap_join(
        color: [u8; 3],
        width: f32,
        cap: VectorStrokeCap,
        join: VectorStrokeJoin,
    ) -> Self {
        Self {
            color,
            width,
            cap,
            join,
        }
    }

    /// Disabled stroke (`width = 0.0`).
    pub const fn none() -> Self {
        Self {
            color: [0, 0, 0],
            width: 0.0,
            cap: VectorStrokeCap::Round,
            join: VectorStrokeJoin::Round,
        }
    }

    pub fn is_visible(&self) -> bool {
        self.width > 1e-4
    }
}

/// Parametric or Bezier path geometry of a [`VectorShape`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VectorGeometry {
    Path {
        #[serde(default)]
        nodes: Vec<VectorPathNode>,
        #[serde(default)]
        closed: bool,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        #[serde(default)]
        corner_radius: f32,
    },
    Ellipse {
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
    },
    Polygon {
        #[serde(default)]
        points: Vec<[f32; 2]>,
        #[serde(default = "default_true")]
        closed: bool,
    },
}

impl VectorGeometry {
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Path { .. } => "path",
            Self::Rect { .. } => "rect",
            Self::Ellipse { .. } => "ellipse",
            Self::Polygon { .. } => "polygon",
        }
    }

    pub const fn is_path(&self) -> bool {
        matches!(self, Self::Path { .. })
    }

    pub const fn is_closed(&self) -> bool {
        match self {
            Self::Path { closed, .. } | Self::Polygon { closed, .. } => *closed,
            Self::Rect { .. } | Self::Ellipse { .. } => true,
        }
    }
}

/// Editable non-destructive vector object on a [`VectorLayerData`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VectorShape {
    pub id: u64,
    pub name: String,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_one_f32")]
    pub opacity: f32,
    pub geometry: VectorGeometry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<[u8; 3]>,
    #[serde(default, deserialize_with = "deserialize_vector_stroke")]
    pub stroke: VectorStroke,
}

impl VectorShape {
    pub fn new(id: u64, name: impl Into<String>, geometry: VectorGeometry) -> Self {
        Self {
            id,
            name: name.into(),
            visible: true,
            opacity: 1.0,
            geometry,
            fill: None,
            stroke: VectorStroke::default(),
        }
    }

    pub fn new_path(
        id: u64,
        name: impl Into<String>,
        nodes: Vec<VectorPathNode>,
        closed: bool,
        fill: Option<[u8; 3]>,
        stroke: VectorStroke,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            visible: true,
            opacity: 1.0,
            geometry: VectorGeometry::Path { nodes, closed },
            fill,
            stroke,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_rect(
        id: u64,
        name: impl Into<String>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        corner_radius: f32,
        fill: Option<[u8; 3]>,
        stroke: VectorStroke,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            visible: true,
            opacity: 1.0,
            geometry: VectorGeometry::Rect {
                x,
                y,
                w,
                h,
                corner_radius,
            },
            fill,
            stroke,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_ellipse(
        id: u64,
        name: impl Into<String>,
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        fill: Option<[u8; 3]>,
        stroke: VectorStroke,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            visible: true,
            opacity: 1.0,
            geometry: VectorGeometry::Ellipse { cx, cy, rx, ry },
            fill,
            stroke,
        }
    }

    pub fn new_polygon(
        id: u64,
        name: impl Into<String>,
        points: Vec<[f32; 2]>,
        closed: bool,
        fill: Option<[u8; 3]>,
        stroke: VectorStroke,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            visible: true,
            opacity: 1.0,
            geometry: VectorGeometry::Polygon { points, closed },
            fill,
            stroke,
        }
    }
}

/// Non-destructive vector scene graph stored on a vector [`Layer`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VectorLayerData {
    #[serde(default)]
    pub shapes: Vec<VectorShape>,
    #[serde(default)]
    pub selected_shape_ids: Vec<u64>,
    #[serde(default = "default_next_shape_id")]
    pub next_shape_id: u64,
}

impl Default for VectorLayerData {
    fn default() -> Self {
        Self::new()
    }
}

impl VectorLayerData {
    pub const fn new() -> Self {
        Self {
            shapes: Vec::new(),
            selected_shape_ids: Vec::new(),
            next_shape_id: 1,
        }
    }

    pub fn from_shapes(shapes: Vec<VectorShape>) -> Self {
        let next_shape_id = shapes
            .iter()
            .map(|s| s.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(1);
        let selected_shape_ids = shapes.last().map(|s| vec![s.id]).unwrap_or_default();
        Self {
            shapes,
            selected_shape_ids,
            next_shape_id,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.shapes.iter().position(|s| s.id == id)
    }

    pub fn shape(&self, id: u64) -> Option<&VectorShape> {
        self.shapes.iter().find(|s| s.id == id)
    }

    pub fn shape_mut(&mut self, id: u64) -> Option<&mut VectorShape> {
        self.shapes.iter_mut().find(|s| s.id == id)
    }

    pub fn allocate_shape_id(&mut self) -> u64 {
        let max_existing = self.shapes.iter().map(|s| s.id).max().unwrap_or(0);
        if self.next_shape_id <= max_existing {
            self.next_shape_id = max_existing + 1;
        }
        let id = self.next_shape_id;
        self.next_shape_id += 1;
        id
    }

    pub fn push_shape(&mut self, mut shape: VectorShape) -> u64 {
        if shape.id == 0 || self.shapes.iter().any(|s| s.id == shape.id) {
            shape.id = self.allocate_shape_id();
        } else if shape.id >= self.next_shape_id {
            self.next_shape_id = shape.id + 1;
        }
        let id = shape.id;
        self.shapes.push(shape);
        self.selected_shape_ids = vec![id];
        id
    }

    pub fn remove_shape(&mut self, id: u64) -> Option<VectorShape> {
        let idx = self.index_of(id)?;
        let removed = self.shapes.remove(idx);
        self.selected_shape_ids.retain(|&sid| sid != id);
        Some(removed)
    }
}

/// Parameters for `Transform Each` across individual vector shapes or layers.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransformEachSpec {
    /// Horizontal scale factor (`1.0` = 100%).
    #[serde(default = "default_one_f32")]
    pub scale_x: f32,
    /// Vertical scale factor (`1.0` = 100%).
    #[serde(default = "default_one_f32")]
    pub scale_y: f32,
    /// Clockwise rotation in degrees around each object's own anchor point.
    #[serde(default)]
    pub rotation_deg: f32,
    /// Horizontal translation in canvas pixels per step.
    #[serde(default)]
    pub translate_x: f32,
    /// Vertical translation in canvas pixels per step.
    #[serde(default)]
    pub translate_y: f32,
    /// Mirror horizontally around each object's own anchor point.
    #[serde(default)]
    pub reflect_x: bool,
    /// Mirror vertically around each object's own anchor point.
    #[serde(default)]
    pub reflect_y: bool,
    /// 9-point anchor on each object's own bounding box.
    #[serde(default)]
    pub anchor: CanvasAnchor,
    /// Number of sequential steps to repeat (`1` = apply once).
    #[serde(default = "default_repeat_steps")]
    pub repeat_steps: u32,
    /// When `true`, preserves original objects and emits transformed copies for steps `1..=repeat_steps`.
    #[serde(default)]
    pub apply_to_duplicate: bool,
}

impl Default for TransformEachSpec {
    fn default() -> Self {
        Self {
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_deg: 0.0,
            translate_x: 0.0,
            translate_y: 0.0,
            reflect_x: false,
            reflect_y: false,
            anchor: CanvasAnchor::Center,
            repeat_steps: 1,
            apply_to_duplicate: false,
        }
    }
}

impl TransformEachSpec {
    pub fn is_identity(&self) -> bool {
        (self.scale_x - 1.0).abs() < 1e-4
            && (self.scale_y - 1.0).abs() < 1e-4
            && self.rotation_deg.abs() < 1e-4
            && self.translate_x.abs() < 1e-4
            && self.translate_y.abs() < 1e-4
            && !self.reflect_x
            && !self.reflect_y
            && !self.apply_to_duplicate
    }
}

/// Parameters for `Vector Blend` (shape/path interpolation over `steps` intermediate steps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VectorBlendSpec {
    pub start_shape_id: u64,
    pub end_shape_id: u64,
    #[serde(default = "default_blend_steps")]
    pub steps: u32,
}

impl Default for VectorBlendSpec {
    fn default() -> Self {
        Self {
            start_shape_id: 1,
            end_shape_id: 2,
            steps: default_blend_steps(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_shape_and_layer_serde_roundtrip() {
        let shape = VectorShape::new_rect(
            1,
            "Rectangle 1",
            10.0,
            20.0,
            100.0,
            60.0,
            8.0,
            Some([240, 80, 60]),
            VectorStroke::with_cap_join(
                [20, 30, 40],
                3.0,
                VectorStrokeCap::Square,
                VectorStrokeJoin::Miter,
            ),
        );
        let layer = VectorLayerData::from_shapes(vec![shape]);
        let json = serde_json::to_string(&layer).expect("serialize VectorLayerData");
        let parsed: VectorLayerData =
            serde_json::from_str(&json).expect("deserialize VectorLayerData");
        assert_eq!(parsed, layer);
        assert_eq!(parsed.next_shape_id, 2);
    }

    #[test]
    fn vector_node_inferred_mode_distinguishes_corner_smooth_symmetric() {
        let corner = VectorPathNode::corner([50.0, 50.0]);
        assert_eq!(corner.inferred_mode(), VectorNodeMode::Corner);

        let sym = VectorPathNode::new([50.0, 50.0], Some([30.0, 50.0]), Some([70.0, 50.0]));
        assert_eq!(sym.inferred_mode(), VectorNodeMode::Symmetric);

        let smooth = VectorPathNode::new([50.0, 50.0], Some([35.0, 50.0]), Some([80.0, 50.0]));
        assert_eq!(smooth.inferred_mode(), VectorNodeMode::Smooth);

        let broken = VectorPathNode::new([50.0, 50.0], Some([35.0, 40.0]), Some([80.0, 50.0]));
        assert_eq!(broken.inferred_mode(), VectorNodeMode::Corner);
    }

    #[test]
    fn layer_summary_deserializes_without_kind_for_backward_compatibility() {
        let legacy_json = r#"{
            "id": 1,
            "name": "Background",
            "visible": true,
            "locked": false,
            "opacity": 1.0,
            "blendMode": "normal",
            "offsetX": 0,
            "offsetY": 0,
            "width": 100,
            "height": 80
        }"#;
        let summary: crate::LayerSummary =
            serde_json::from_str(legacy_json).expect("deserialize legacy LayerSummary");
        assert_eq!(summary.kind, LayerKind::Raster);
    }
}
