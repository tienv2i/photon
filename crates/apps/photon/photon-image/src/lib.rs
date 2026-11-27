//! Qt-independent image decoding, header probing, multi-layer compositing,
//! parallel pixel operations, liquify warps, and non-destructive editor document pipeline.

pub mod align;
pub mod archive;
pub mod codec;
pub mod document;
pub mod drawing;
pub mod healing;
pub mod layer;
pub mod liquify;
mod native_codecs;
pub mod ora;
pub mod pixels;
pub mod selection;
pub mod text;
pub mod tile;
pub mod vector;
pub mod vector_boolean;

use photon_protocol::UiText;
use thiserror::Error;

pub use align::{AlignItem, compute_align_deltas, compute_distribute_deltas, union_bounds};
pub use codec::{decode_rgba, generate_thumbnail, probe_dimensions};
pub use document::{EditOp, EditorDocument, render_single_op, validate_dimensions};
pub use drawing::{
    GradientBlendMode, GradientRepeatSpec, GradientShapeSpec, GradientStopSpec, render_gradient,
    render_gradient_on_mask, stroke_polyline_aa,
};
pub use healing::healing_stroke;
pub use layer::{
    Layer, LayerOperation, LayerStack, LayerThumbnail, blend_layer_onto, composite_region,
    composite_stack, composite_stack_excluding, rasterize_layer, render_layer_thumbnails,
};
pub use selection::{SelectionMask, SelectionSnapshot, rasterize_shape};
pub use text::{
    available_font_families, measure_text_bounds, rasterize_text_layer, system_fontdb,
    wrap_text_lines,
};
pub use tile::{DirtyRect, SURFACE_TILE_SIZE, TileId, TiledSurface};
pub use vector::{
    add_node_at_closest_point, add_node_on_segment, anchor_point_for_bounds, apply_blend_to_layer,
    apply_transform_each_to_layer, blend_vector_shapes, convert_geometry_to_path,
    convert_shape_to_path, delete_node, delete_nodes, geometry_to_bezier_path,
    geometry_to_polyline, join_paths, join_shapes, move_node_anchor, move_node_handle,
    render_vector_layer, render_vector_shape_onto, resample_bezier_path, reverse_path_nodes,
    reverse_shape_path, set_node_mode, set_path_closed, shape_bounds, simplify_path_nodes,
    simplify_shape, smooth_path_nodes, smooth_shape, split_path_at_node, split_shape_at_node,
    toggle_path_closed, transform_each_shapes, transform_shape_once, vector_layer_to_svg,
};
pub use vector_boolean::{
    apply_knife_to_layer, apply_vector_boolean_to_layer, boolean_polygons, knife_split_polyline,
};

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
