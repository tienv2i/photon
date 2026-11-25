//! Reusable pure-`iced` Design System & UI Component Kit (`photon-ui`).
//!
//! Zero domain dependencies: provides neutral studio color tokens (Dark/Light),
//! vector SVG icons, buttons, panels, menus, modal dialogs, and toolbars.

pub mod button;
pub mod curve_editor;
pub mod dialog;
pub mod editor;
pub mod grid;
pub mod icons;
pub mod menu;
pub mod panel;
pub mod splitter;
pub mod table;
pub mod theme;
pub mod toolbar;
pub mod tooltip;

pub use curve_editor::{CurveGraphEvent, CurveGraphProgram, curve_graph, curve_graph_sized};
pub use icons::Icon;
pub use theme::{PaletteTokens, UiMode};
pub use tooltip::{TooltipPosition, with_optional_tooltip, with_tooltip, with_tooltip_pos};
