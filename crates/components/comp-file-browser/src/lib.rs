//! Reusable, domain-agnostic File Browser component:
//! formatting, filtering (plain / case-sensitive / regex), thumbnail sizing,
//! sorting, and virtualized grid/list viewport range math.

pub mod model;
pub mod scanner;
pub mod viewport;

pub use model::{
    FileEntryLike, SortField, SortOrder, ThumbSize, compile_filter_regex, entry_kind_label,
    entry_name_matches_filter, format_bytes, format_modified_ms, sort_file_entries,
    step_thumb_size, thumb_card_min_width, thumb_size_label,
};
pub use scanner::{
    FilesystemProfile, ScannedEntry, detect_filesystem_profile, scan_dir_fast, stat_entry_metadata,
};
pub use viewport::{
    GRID_CARD_META_HEIGHT, GRID_CONTENT_TOP, LIST_CONTENT_TOP, LIST_ROW_HEIGHT, grid_visible_range,
    list_visible_range, visible_linear_range,
};
