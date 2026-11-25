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

fn default_brush_hardness() -> f32 {
    0.85
}

fn default_brush_spacing() -> f32 {
    0.15
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
        }
    }
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
                | Self::Invert
                | Self::Desaturate
                | Self::Sepia
        )
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
