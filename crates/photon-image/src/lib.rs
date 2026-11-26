//! Qt-independent image decoding, header probing, multi-layer compositing,
//! parallel pixel operations, liquify warps, and non-destructive editor document pipeline.

pub mod codec;
pub mod document;
pub mod drawing;
pub mod healing;
pub mod layer;
pub mod liquify;
pub mod ora;
pub mod pixels;
pub mod selection;

use photon_protocol::UiText;
use thiserror::Error;

pub use codec::{decode_rgba, generate_thumbnail, probe_dimensions};
pub use document::{EditOp, EditorDocument, render_single_op, validate_dimensions};
pub use drawing::{
    GradientBlendMode, GradientRepeatSpec, GradientShapeSpec, GradientStopSpec, render_gradient,
    render_gradient_on_mask,
};
pub use healing::healing_stroke;
pub use layer::{
    Layer, LayerOperation, LayerStack, LayerThumbnail, blend_layer_onto, composite_stack,
    composite_stack_excluding, render_layer_thumbnails,
};
pub use selection::{SelectionMask, SelectionSnapshot, rasterize_shape};

pub const MAX_IMAGE_DIMENSION: u32 = 32_768;
pub const MAX_DOCUMENT_PIXELS: u64 = 100_000_000;
pub const DEFAULT_TILE_SIZE: u32 = 2048;

pub const MAX_HISTORY_STEPS: usize = 30;
pub const MAX_SNAPSHOT_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ImageError {
    #[error("cannot open image {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("decode failed for {path}: {message}")]
    Decode { path: String, message: String },
    #[error("encode failed for {path}: {message}")]
    Encode { path: String, message: String },
    #[error("invalid edit: {0}")]
    InvalidEdit(String),
    #[error("{code}")]
    Coded { code: &'static str, ui: UiText },
}

impl ImageError {
    pub fn coded(code: &'static str) -> Self {
        Self::Coded {
            code,
            ui: UiText::new(code),
        }
    }
}
