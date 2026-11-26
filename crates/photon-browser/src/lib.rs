//! Advanced Browser workspace UI (`iced 0.14` + `photon-ui`):
//! - Resizable Quick Access (`favorites`) + Explorer sidebar (`panel_splitter`)
//! - Sortable header (`Name` / `Modified` / `Size` / `Type` + `Asc`/`Desc`),
//!   Thumbnail Size stepper (`XS`..`XL`), `Grid` | `List` | `Details` views
//! - Live Batch progress banner & Batch launcher menu (`Resize…` / `Crop…` / `Convert…`)
//! - Right-click context menus (`BrowserEntry` & `BrowserQuickAccess`)

pub mod batch_dialog;
mod menus;
mod model;
mod plugins;
mod sidebars;
mod state;
mod toolbar;
mod views;

pub use menus::{
    build_browser_context_menu_entries, build_browser_context_menu_entries_with_offset,
    build_browser_edit_menu_entries, build_browser_help_menu_entries,
    build_browser_plugins_menu_entries, build_browser_sort_menu, build_browser_window_menu_entries,
    build_quick_access_context_menu,
};
pub use model::{
    entry_kind_label, entry_name_matches_filter, format_bytes, format_modified_ms,
    thumb_card_min_width,
};
pub use plugins::{
    build_browser_plugin_group_submenu, build_browser_plugins_submenu,
    build_browser_plugins_submenu_with_offset, normalize_plugin_action_labels,
    normalize_plugin_display_name, workspace_quick_actions,
};
pub use state::{BrowserMenuKind, BrowserMessage, BrowserViewState};
pub use toolbar::{browser_filter_input_id, build_browser_toolbar};
pub use views::{build_browser_view, grid_visible_range, list_visible_range, visible_linear_range};
