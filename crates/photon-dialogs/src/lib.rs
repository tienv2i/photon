//! Unified OS native system dialogs (`rfd`) and interactive `iced 0.14` custom overlays,
//! movable/resizable dialog frames, and live-preview canvas cutout support.

pub mod frame;
pub mod manager;
pub mod overlay;
pub mod system;

pub use frame::{DialogStyle, ManagedDialogSpec, render_managed_dialog};
pub use manager::{
    ActiveGesture, DialogEvent, DialogManager, DialogWindowState, DragOp, ResizeEdge,
    apply_move_delta, apply_resize_delta, clamp_dialog_rect,
};
pub use overlay::{CutoutStrips, OverlayMode, apply_overlay, compute_cutout_strips};
pub use system::{
    FileFilter, IMAGE_EXTENSIONS, open_file, open_files, open_images, pick_folder, save_file,
    save_image_as,
};
