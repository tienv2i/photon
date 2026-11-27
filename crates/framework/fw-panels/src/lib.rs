//! Unified dockable, collapsible, closable, and drag-reorderable sidebar panel system.
//!
//! Built atop `fw-ui` design tokens, icons, tooltips, and splitter primitives so
//! Browser, Viewer, and Editor sidebars share identical panel chrome and drag-and-drop ordering.

pub mod frame;
pub mod header;
pub mod order;
pub mod splitter;
pub mod tabs;

pub use frame::{
    DockPanelSpec, PanelFrameSpec, dock_panel, dock_panel_closable, drop_target_panel,
    drop_target_panel_fill, kv_row, render_dock_panel, section_card, sidebar_dock, sidebar_panel,
    status_bar,
};
pub use header::{DraggablePanelHeaderSpec, draggable_panel_header};
pub use order::{
    DropIndicator, SidebarOrderState, drop_indicator_for, normalize_panel_order,
    reorder_panel_slice,
};
pub use splitter::{
    PanelSplitter, PanelSplitterEdge, panel_splitter, split_portions, stacked_panel_heights,
    vertical_panel_splitter,
};
pub use tabs::{
    DocumentTabItem, PanelTabItem, document_tab_bar, inline_panel_tab_bar, panel_tab_bar,
};
