//! Editor protocol types: brushes, adjustments, layer summaries, geometry modes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ColorAdjustments;

pub(crate) fn default_true() -> bool {
    true
}

pub(crate) fn default_one_f32() -> f32 {
    1.0
}

pub(crate) fn default_half_f32() -> f32 {
    0.5
}

pub(crate) fn default_opaque_alpha() -> u8 {
    255
}

pub(crate) fn default_brush_hardness() -> f32 {
    0.85
}

fn default_brush_spacing() -> f32 {
    0.15
}

fn default_stabilizer_distance() -> f32 {
    24.0
}

/// How `EditorResize` relates the source's aspect ratio to the target dimensions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResizeMode {
    #[default]
    Stretch,
    Cover,
    Pad,
}

/// 9-point anchor for `EditorCanvasSize` placement.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CanvasAnchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    #[default]
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl CanvasAnchor {
    pub const ALL: [Self; 9] = [
        Self::TopLeft,
        Self::Top,
        Self::TopRight,
        Self::Left,
        Self::Center,
        Self::Right,
        Self::BottomLeft,
        Self::Bottom,
        Self::BottomRight,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TopLeft => "top_left",
            Self::Top => "top",
            Self::TopRight => "top_right",
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
            Self::BottomLeft => "bottom_left",
            Self::Bottom => "bottom",
            Self::BottomRight => "bottom_right",
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::TopLeft => 0,
            Self::Top => 1,
            Self::TopRight => 2,
            Self::Left => 3,
            Self::Center => 4,
            Self::Right => 5,
            Self::BottomLeft => 6,
            Self::Bottom => 7,
            Self::BottomRight => 8,
        }
    }

    pub const fn from_index(idx: usize) -> Self {
        match idx {
            0 => Self::TopLeft,
            1 => Self::Top,
            2 => Self::TopRight,
            3 => Self::Left,
            4 => Self::Center,
            5 => Self::Right,
            6 => Self::BottomLeft,
            7 => Self::Bottom,
            8 => Self::BottomRight,
            _ => Self::Center,
        }
    }
}

/// Which target(s) an editor operation modifies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApplyTarget {
    ActiveLayer,
    #[default]
    WholeImage,
    NewLayer,
}

/// W3C layer compositing blend mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
}

impl BlendMode {
    pub const ALL: [Self; 7] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Darken,
        Self::Lighten,
        Self::Difference,
    ];

    pub const fn as_key(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Overlay => "overlay",
            Self::Darken => "darken",
            Self::Lighten => "lighten",
            Self::Difference => "difference",
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "multiply" => Self::Multiply,
            "screen" => Self::Screen,
            "overlay" => Self::Overlay,
            "darken" => Self::Darken,
            "lighten" => Self::Lighten,
            "difference" => Self::Difference,
            _ => Self::Normal,
        }
    }
}

/// Brush tip profile for `EditorPaintStroke` and live strokes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrushTip {
    #[default]
    Round,
    Flat,
    Chalk,
    Spray,
    Pixel,
}

impl BrushTip {
    pub const ALL: [Self; 5] = [
        Self::Round,
        Self::Flat,
        Self::Chalk,
        Self::Spray,
        Self::Pixel,
    ];
}

/// Stroke smoothing mode for brush and path strokes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrushSmoothingMode {
    #[default]
    None,
    Basic,
    Weighted,
    Stabilizer,
}

impl BrushSmoothingMode {
    pub const ALL: [Self; 4] = [Self::None, Self::Basic, Self::Weighted, Self::Stabilizer];
}

/// Brush dynamics attached to a paint or erase stroke.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BrushSettings {
    #[serde(default)]
    pub tip: BrushTip,
    #[serde(default = "default_brush_hardness")]
    pub hardness: f32,
    #[serde(default = "default_one_f32")]
    pub flow: f32,
    #[serde(default = "default_brush_spacing")]
    pub spacing: f32,
    #[serde(default)]
    pub angle: f32,
    #[serde(default)]
    pub scatter: f32,
    #[serde(default)]
    pub smoothing_mode: BrushSmoothingMode,
    #[serde(default = "default_stabilizer_distance")]
    pub stabilizer_distance: f32,
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            tip: BrushTip::Round,
            hardness: default_brush_hardness(),
            flow: 1.0,
            spacing: default_brush_spacing(),
            angle: 0.0,
            scatter: 0.0,
            smoothing_mode: BrushSmoothingMode::None,
            stabilizer_distance: default_stabilizer_distance(),
        }
    }
}

/// Cubic Bezier anchor and optional control handles for vector/bezier paths.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VectorPathNode {
    pub anchor: [f32; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle_in: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle_out: Option<[f32; 2]>,
}

/// Shape kind for `EditorDrawShape`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DrawShapeKind {
    #[default]
    Line,
    Rect,
    Ellipse,
    Polygon,
    Polyline,
    BezierPath,
}

/// Stroke/fill mode for `EditorDrawShape`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShapeFillMode {
    #[default]
    Stroke,
    Fill,
    StrokeAndFill,
}

/// Gradient geometry for `EditorDrawGradient`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GradientKind {
    #[default]
    Linear,
    Radial,
    Conical,
    Reflected,
}

/// Gradient color ramp mode for `EditorDrawGradient`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GradientColorMode {
    #[default]
    FgToBg,
    FgToTransparent,
}

/// Liquify warp brush mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiquifyMode {
    #[default]
    Push,
    Bloat,
    Pucker,
    TwirlCw,
    TwirlCcw,
    Reconstruct,
}

impl LiquifyMode {
    pub const ALL: [Self; 6] = [
        Self::Push,
        Self::Bloat,
        Self::Pucker,
        Self::TwirlCw,
        Self::TwirlCcw,
        Self::Reconstruct,
    ];
}

/// Direction for reordering favorite Quick Access folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MoveDirection {
    Up,
    Down,
}

/// Healing brush operating mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealingMode {
    #[default]
    Spot,
    Sampled,
}

impl HealingMode {
    pub const ALL: [Self; 2] = [Self::Spot, Self::Sampled];
}

/// Interactive live stroke kind for `EditorLiveStrokeBegin`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveStrokeKind {
    Paint {
        color: [u8; 3],
        radius: f32,
        #[serde(default = "default_one_f32")]
        opacity: f32,
        #[serde(default)]
        erase: bool,
        #[serde(flatten, default)]
        brush: BrushSettings,
    },
    Liquify {
        #[serde(default)]
        mode: LiquifyMode,
        radius: f32,
        #[serde(default = "default_half_f32")]
        strength: f32,
    },
    Healing {
        #[serde(default)]
        mode: HealingMode,
        radius: f32,
        #[serde(default = "default_brush_hardness")]
        hardness: f32,
        #[serde(default = "default_one_f32")]
        strength: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<[f64; 2]>,
    },
}

/// HSV color adjustment parameters (`hue` in `-180..=180`, `saturation` & `value` in `-1..=1`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HsvAdjustment {
    #[serde(default)]
    pub hue: f32,
    #[serde(default)]
    pub saturation: f32,
    #[serde(default)]
    pub value: f32,
}

/// Tonal Color Balance parameters across Shadows, Midtones, and Highlights.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ColorBalanceAdjustment {
    #[serde(default)]
    pub shadows: [f32; 3],
    #[serde(default)]
    pub midtones: [f32; 3],
    #[serde(default)]
    pub highlights: [f32; 3],
    #[serde(default = "default_true")]
    pub preserve_luminosity: bool,
}

impl Default for ColorBalanceAdjustment {
    fn default() -> Self {
        Self {
            shadows: [0.0; 3],
            midtones: [0.0; 3],
            highlights: [0.0; 3],
            preserve_luminosity: true,
        }
    }
}

/// Per-channel tonal curve control points in normalized `0.0..=1.0` coordinates.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CurvesAdjustment {
    #[serde(default)]
    pub rgb: Vec<[f32; 2]>,
    #[serde(default)]
    pub red: Vec<[f32; 2]>,
    #[serde(default)]
    pub green: Vec<[f32; 2]>,
    #[serde(default)]
    pub blue: Vec<[f32; 2]>,
}

/// Non-destructive Filter or Adjustment effect hosted on a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "effectKind", rename_all = "snake_case")]
pub enum LayerEffect {
    ColorLight(ColorAdjustments),
    Hsv(HsvAdjustment),
    ColorBalance(ColorBalanceAdjustment),
    Curves(CurvesAdjustment),
    Blur { radius: f32 },
    Sharpen { amount: f32 },
    Denoise { strength: f32, detail_preserve: f32 },
    Invert,
    Desaturate,
    Sepia,
}

impl LayerEffect {
    pub fn default_name(&self) -> &'static str {
        match self {
            Self::ColorLight(_) => "Color & Light",
            Self::Hsv(_) => "HSV Adjustment",
            Self::ColorBalance(_) => "Color Balance",
            Self::Curves(_) => "Curves",
            Self::Blur { .. } => "Gaussian Blur",
            Self::Sharpen { .. } => "Sharpen",
            Self::Denoise { .. } => "Denoise",
            Self::Invert => "Invert",
            Self::Desaturate => "Black & White",
            Self::Sepia => "Sepia",
        }
    }

    pub fn is_filter(&self) -> bool {
        matches!(
            self,
            Self::Blur { .. }
                | Self::Sharpen { .. }
                | Self::Denoise { .. }
                | Self::Invert
                | Self::Desaturate
                | Self::Sepia
        )
    }
}

/// 2D affine transform parameters applied to a layer or selection region.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct LayerTransformSpec {
    pub translate_x: f32,
    pub translate_y: f32,
    #[serde(default = "default_one_f32")]
    pub scale_x: f32,
    #[serde(default = "default_one_f32")]
    pub scale_y: f32,
    pub rotation_deg: f32,
    #[serde(default = "default_half_f32")]
    pub pivot_x: f32,
    #[serde(default = "default_half_f32")]
    pub pivot_y: f32,
    pub flip_h: bool,
    pub flip_v: bool,
}

impl Default for LayerTransformSpec {
    fn default() -> Self {
        Self {
            translate_x: 0.0,
            translate_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_deg: 0.0,
            pivot_x: 0.5,
            pivot_y: 0.5,
            flip_h: false,
            flip_v: false,
        }
    }
}

impl LayerTransformSpec {
    pub fn is_identity(&self) -> bool {
        self.translate_x.abs() < 1e-4
            && self.translate_y.abs() < 1e-4
            && (self.scale_x - 1.0).abs() < 1e-4
            && (self.scale_y - 1.0).abs() < 1e-4
            && self.rotation_deg.abs() < 1e-4
            && !self.flip_h
            && !self.flip_v
    }
}

/// Lightweight serializable summary of one layer in `EditorState`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LayerSummary {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub offset_x: i32,
    pub offset_y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub has_mask: bool,
    #[serde(default = "default_true")]
    pub mask_enabled: bool,
    #[serde(default)]
    pub editing_mask: bool,
    #[serde(default = "default_true")]
    pub mask_linked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<LayerEffect>,
}

/// Boolean combination mode for pixel selections (Photoshop + Krita).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SelectionBooleanOp {
    #[default]
    Replace,
    Add,
    Subtract,
    Intersect,
    SymmetricDiff,
}

/// Geometric, path, or color-guided pixel selection input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SelectionShape {
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        #[serde(default)]
        feather: f32,
        #[serde(default = "default_true")]
        anti_alias: bool,
    },
    Ellipse {
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        #[serde(default)]
        feather: f32,
        #[serde(default = "default_true")]
        anti_alias: bool,
    },
    SingleRow {
        y: u32,
    },
    SingleCol {
        x: u32,
    },
    Polygon {
        points: Vec<[f32; 2]>,
        #[serde(default)]
        feather: f32,
        #[serde(default = "default_true")]
        anti_alias: bool,
    },
    MagicWand {
        x: u32,
        y: u32,
        tolerance: u8,
        #[serde(default = "default_true")]
        contiguous: bool,
        #[serde(default)]
        sample_all_layers: bool,
        #[serde(default = "default_true")]
        anti_alias: bool,
    },
    ColorRange {
        rgb: [u8; 3],
        fuzziness: u8,
        #[serde(default = "default_true")]
        sample_all_layers: bool,
    },
    QuickSelectStroke {
        points: Vec<[f64; 2]>,
        radius: f32,
        #[serde(default = "default_true")]
        sample_all_layers: bool,
    },
}

/// Morphological or color-expansion modification on the active selection.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SelectionModifyOp {
    Feather(f32),
    Expand(u32),
    Contract(u32),
    Border(u32),
    Smooth(u32),
    Grow(u8),
    Similar(u8),
}

/// Capabilities declared by a Photon external plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    EditorFilter,
    BrowserAction,
}

/// Parameter widget/value kind in a plugin parameter schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluginParamKind {
    Float,
    Int,
    Bool,
    Select,
    Color,
}

/// Typed runtime value for a plugin parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum PluginParamValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Color([u8; 3]),
    Select(String),
}

/// Declarative parameter specification in `plugin.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginParamSpec {
    pub key: String,
    pub label: String,
    pub kind: PluginParamKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<PluginParamValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Parsed `plugin.json` manifest for a process-isolated NDJSON plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    pub entry: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<PluginCapability>,
    #[serde(default)]
    pub params: Vec<PluginParamSpec>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// Published summary of an installed plugin in `UiStateMeta`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    pub enabled: bool,
    #[serde(default)]
    pub capabilities: Vec<PluginCapability>,
    #[serde(default)]
    pub params: Vec<PluginParamSpec>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub install_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl PluginInfo {
    pub fn from_manifest(
        manifest: &PluginManifest,
        enabled: bool,
        install_dir: impl Into<String>,
        error: Option<String>,
    ) -> Self {
        Self {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            author: manifest.author.clone(),
            enabled,
            capabilities: manifest.capabilities.clone(),
            params: manifest.params.clone(),
            permissions: manifest.permissions.clone(),
            install_dir: install_dir.into(),
            error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_spec_defaults_to_identity() {
        let spec = LayerTransformSpec::default();
        assert_eq!(spec.scale_x, 1.0);
        assert_eq!(spec.scale_y, 1.0);
        assert_eq!(spec.pivot_x, 0.5);
        assert_eq!(spec.pivot_y, 0.5);
        assert!(spec.is_identity());

        let parsed: LayerTransformSpec = serde_json::from_str(r#"{"rotationDeg":15.0}"#).unwrap();
        assert_eq!(parsed.scale_x, 1.0);
        assert_eq!(parsed.pivot_x, 0.5);
        assert!(!parsed.is_identity());
    }

    #[test]
    fn plugin_manifest_and_healing_types_roundtrip() {
        let json = r#"{
            "id": "com.example.demo",
            "name": "Demo",
            "version": "1.0.0",
            "entry": "run.py",
            "capabilities": ["editor_filter"],
            "params": [
                {"key": "strength", "label": "Strength", "kind": "float", "default": 0.5, "min": 0.0, "max": 1.0},
                {"key": "invert", "label": "Invert", "kind": "bool", "default": true}
            ],
            "permissions": ["read_image"]
        }"#;
        let manifest: PluginManifest = serde_json::from_str(json).unwrap();
        let info = PluginInfo::from_manifest(&manifest, true, "/tmp/demo", None);
        assert_eq!(info.id, "com.example.demo");
        assert_eq!(info.params.len(), 2);
        assert_eq!(info.params[0].default, Some(PluginParamValue::Float(0.5)));
        assert_eq!(info.params[1].default, Some(PluginParamValue::Bool(true)));

        let stroke = LiveStrokeKind::Healing {
            mode: HealingMode::Sampled,
            radius: 12.0,
            hardness: 0.7,
            strength: 0.9,
            source: Some([10.0, 20.0]),
        };
        let round: LiveStrokeKind =
            serde_json::from_str(&serde_json::to_string(&stroke).unwrap()).unwrap();
        assert_eq!(round, stroke);

        let effect = LayerEffect::Denoise {
            strength: 0.4,
            detail_preserve: 0.6,
        };
        assert!(effect.is_filter());
        assert_eq!(effect.default_name(), "Denoise");
    }
}
