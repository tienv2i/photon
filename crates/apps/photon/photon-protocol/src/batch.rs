//! Typed batch processing protocol types shared across Core, GUI, CLI, and MCP.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{CanvasAnchor, ResizeFilter, ResizeMode, TransformEachSpec, default_opaque_alpha};

/// Output file format for `BatchOperation::Convert`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BatchImageFormat {
    Png,
    Jpeg,
    Webp,
    Tiff,
    Gif,
    Bmp,
    Tga,
    Qoi,
}

impl BatchImageFormat {
    pub const ALL: [Self; 8] = [
        Self::Png,
        Self::Jpeg,
        Self::Webp,
        Self::Tiff,
        Self::Gif,
        Self::Bmp,
        Self::Tga,
        Self::Qoi,
    ];

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Tiff => "tiff",
            Self::Gif => "gif",
            Self::Bmp => "bmp",
            Self::Tga => "tga",
            Self::Qoi => "qoi",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Png => ".png",
            Self::Jpeg => ".jpg",
            Self::Webp => ".webp",
            Self::Tiff => ".tiff",
            Self::Gif => ".gif",
            Self::Bmp => ".bmp",
            Self::Tga => ".tga",
            Self::Qoi => ".qoi",
        }
    }
}

/// Unit for `BatchOperation::Crop`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BatchCropUnit {
    #[default]
    Percent,
    Pixels,
}

/// Operation applied to every image in a batch job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum BatchOperation {
    Resize {
        width: u32,
        height: u32,
        #[serde(default)]
        mode: ResizeMode,
        #[serde(default)]
        filter: ResizeFilter,
        #[serde(default)]
        fill: Option<[u8; 3]>,
        #[serde(default = "default_opaque_alpha")]
        fill_alpha: u8,
    },
    Crop {
        #[serde(default)]
        x: f64,
        #[serde(default)]
        y: f64,
        #[serde(default)]
        width: f64,
        #[serde(default)]
        height: f64,
        #[serde(default)]
        unit: BatchCropUnit,
        #[serde(default)]
        anchor: Option<CanvasAnchor>,
        #[serde(default)]
        ratio: Option<f64>,
    },
    Convert {
        format: BatchImageFormat,
        #[serde(default)]
        quality: Option<u8>,
    },
    Transform {
        spec: TransformEachSpec,
    },
}

impl BatchOperation {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Resize { .. } => "resize",
            Self::Crop { .. } => "crop",
            Self::Convert { .. } => "convert",
            Self::Transform { .. } => "transform",
        }
    }
}

/// Input set for a batch job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum BatchSource {
    Selection { paths: Vec<String> },
    Folder { path: String },
}

/// Destination policy for a batch job.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchOutput {
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default)]
    pub suffix: String,
    #[serde(default)]
    pub overwrite: bool,
}

/// Live batch job progress snapshot projected in `UiStateMeta`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchState {
    pub job_id: String,
    pub operation: String,
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub running: bool,
    pub cancelled: bool,
    pub current: String,
}
