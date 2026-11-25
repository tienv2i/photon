//! Semantic UI registry & presentation snapshot builder for agent inspection (`debug ui-tree`).

use photon_i18n::tr;
use photon_protocol::{
    BrowserViewMode, PresentationState, SemanticWidgetNode, UiStateSnapshot, UiTreeSnapshot,
    ViewerViewMode, Workspace,
};

use crate::app::ActiveContextMenu;
use photon_editor::{AdjustmentPopupTab, CompareMode, EditorTool, EditorUiState};

pub struct ViewerUiPresentation {
    pub view_mode: ViewerViewMode,
    pub info_open: bool,
    pub adjustments_open: bool,
    pub filmstrip_open: bool,
    pub is_fullscreen: bool,
    pub color_popover_open: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn build_ui_tree(
    snapshot: &UiStateSnapshot,
    window_count: usize,
    window_size: iced::Size,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    gpu_backend: &str,
    viewer_ui: &ViewerUiPresentation,
    editor_ui: &EditorUiState,
    context_menu: Option<&ActiveContextMenu>,
) -> UiTreeSnapshot {
    let lang = snapshot.meta.language;
    let ws = snapshot.meta.workspace;

    let mut widgets = vec![
        SemanticWidgetNode {
            id: "toolbar.theme_toggle".to_owned(),
            role: "button".to_owned(),
            label: tr(lang, "action-theme-toggle"),
            workspace: ws,
            enabled: true,
            visible: true,
            bounds: None,
            value: Some(snapshot.meta.theme.as_str().to_owned()),
        },
        SemanticWidgetNode {
            id: "toolbar.settings".to_owned(),
            role: "button".to_owned(),
            label: tr(lang, "action-settings"),
            workspace: ws,
            enabled: true,
            visible: true,
            bounds: None,
            value: None,
        },
    ];

    if ws != Workspace::Browser {
        widgets.push(SemanticWidgetNode {
            id: "nav.back".to_owned(),
            role: "button".to_owned(),
            label: tr(
                lang,
                if ws == Workspace::Editor && snapshot.meta.viewer_count > 0 {
                    "workspace-viewer"
                } else {
                    "workspace-browser"
                },
            ),
            workspace: ws,
            enabled: true,
            visible: true,
            bounds: None,
            value: None,
        });
    }

    match ws {
        Workspace::Browser => {
            widgets.push(SemanticWidgetNode {
                id: "toolbar.refresh".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-refresh"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.explorer".to_owned(),
                role: "tree".to_owned(),
                label: tr(lang, "label-explorer"),
                workspace: ws,
                enabled: snapshot.meta.root_folder.is_some(),
                visible: true,
                bounds: None,
                value: snapshot.meta.root_folder.clone(),
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.breadcrumbs".to_owned(),
                role: "nav".to_owned(),
                label: tr(lang, "label-folders"),
                workspace: ws,
                enabled: !snapshot.meta.breadcrumbs.is_empty(),
                visible: !snapshot.meta.breadcrumbs.is_empty(),
                bounds: None,
                value: snapshot.meta.current_folder.clone(),
            });
            let layout = &snapshot.meta.layout;
            widgets.push(SemanticWidgetNode {
                id: "browser.quick_access.list".to_owned(),
                role: "list".to_owned(),
                label: tr(lang, "label-quick-access"),
                workspace: ws,
                enabled: true,
                visible: layout.panel("browser.sidebar"),
                bounds: None,
                value: Some(format!("{} favorites", snapshot.meta.favorites.len())),
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.splitter.left".to_owned(),
                role: "splitter".to_owned(),
                label: "Left Splitter".to_owned(),
                workspace: ws,
                enabled: layout.panel("browser.sidebar"),
                visible: layout.panel("browser.sidebar"),
                bounds: None,
                value: Some(layout.left_width.to_string()),
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.splitter.right".to_owned(),
                role: "splitter".to_owned(),
                label: "Right Splitter".to_owned(),
                workspace: ws,
                enabled: layout.panel("browser.inspector"),
                visible: layout.panel("browser.inspector"),
                bounds: None,
                value: Some(layout.right_width.to_string()),
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.view_mode".to_owned(),
                role: "toggle".to_owned(),
                label: tr(lang, "action-view-grid"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: Some(format!("{:?}", layout.browser_view_mode)),
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.details.table".to_owned(),
                role: "table".to_owned(),
                label: tr(lang, "action-view-details"),
                workspace: ws,
                enabled: true,
                visible: layout.browser_view_mode == BrowserViewMode::Details,
                bounds: None,
                value: Some(format!("{} entries", snapshot.entries.len())),
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.batch.banner".to_owned(),
                role: "status".to_owned(),
                label: tr(lang, "batch-progress"),
                workspace: ws,
                enabled: snapshot.meta.batch.is_some(),
                visible: snapshot.meta.batch.is_some(),
                bounds: None,
                value: snapshot
                    .meta
                    .batch
                    .as_ref()
                    .map(|b| format!("{}/{}", b.done, b.total)),
            });
            let has_images = snapshot.entries.iter().any(|e| e.is_image);
            let has_selected_image = snapshot
                .entries
                .iter()
                .any(|e| e.is_image && snapshot.meta.selection.contains(&e.path));
            widgets.push(SemanticWidgetNode {
                id: "browser.menu.file".to_owned(),
                role: "menu_button".to_owned(),
                label: tr(lang, "menu-file"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.menu.file.edit".to_owned(),
                role: "menu_item".to_owned(),
                label: tr(lang, "menu-file-edit"),
                workspace: ws,
                enabled: has_selected_image,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.menu.view".to_owned(),
                role: "menu_button".to_owned(),
                label: tr(lang, "menu-view"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.menu.view.detail_panel".to_owned(),
                role: "menu_item".to_owned(),
                label: tr(lang, "menu-view-detail-panel"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: Some(layout.panel("browser.inspector").to_string()),
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.menu.tools".to_owned(),
                role: "menu_button".to_owned(),
                label: tr(lang, "menu-tools"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            for (id, label_key, enabled) in [
                (
                    "browser.menu.tools.batch_resize",
                    "menu-tools-batch-resize",
                    has_images,
                ),
                (
                    "browser.menu.tools.batch_crop",
                    "menu-tools-batch-crop",
                    has_images,
                ),
                (
                    "browser.menu.tools.batch_convert",
                    "menu-tools-batch-convert",
                    has_images,
                ),
                (
                    "browser.menu.tools.edit",
                    "menu-tools-edit",
                    has_selected_image,
                ),
            ] {
                widgets.push(SemanticWidgetNode {
                    id: id.to_owned(),
                    role: "menu_item".to_owned(),
                    label: tr(lang, label_key),
                    workspace: ws,
                    enabled,
                    visible: true,
                    bounds: None,
                    value: None,
                });
            }
            widgets.push(SemanticWidgetNode {
                id: "browser.menu.plugins".to_owned(),
                role: "menu_button".to_owned(),
                label: tr(lang, "menu-plugins"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.menu.plugins.manage".to_owned(),
                role: "menu_item".to_owned(),
                label: tr(lang, "action-manage-plugins"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.menu.plugins.reload".to_owned(),
                role: "menu_item".to_owned(),
                label: tr(lang, "action-reload-plugins"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.menu.window".to_owned(),
                role: "menu_button".to_owned(),
                label: tr(lang, "menu-window"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            for (id, label_key, checked) in [
                (
                    "browser.menu.window.quick_access",
                    "panel-quick-access",
                    layout.panel("browser.quick_access"),
                ),
                (
                    "browser.menu.window.explorer",
                    "panel-explorer",
                    layout.panel("browser.explorer"),
                ),
                (
                    "browser.menu.window.detail",
                    "menu-view-detail-panel",
                    layout.panel("browser.inspector"),
                ),
                (
                    "browser.menu.window.preview",
                    "panel-preview",
                    layout.panel("browser.preview"),
                ),
                (
                    "browser.menu.window.info",
                    "panel-info",
                    layout.panel("browser.info"),
                ),
            ] {
                widgets.push(SemanticWidgetNode {
                    id: id.to_owned(),
                    role: "menu_item".to_owned(),
                    label: tr(lang, label_key),
                    workspace: ws,
                    enabled: true,
                    visible: true,
                    bounds: None,
                    value: Some(checked.to_string()),
                });
            }
            widgets.push(SemanticWidgetNode {
                id: "browser.menu.help".to_owned(),
                role: "menu_button".to_owned(),
                label: tr(lang, "menu-help"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.filter_toggle".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "tooltip-browser-filter-toggle"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.quick_access".to_owned(),
                role: "panel".to_owned(),
                label: tr(lang, "panel-quick-access"),
                workspace: ws,
                enabled: layout.panel("browser.sidebar"),
                visible: layout.panel("browser.sidebar") && layout.panel("browser.quick_access"),
                bounds: None,
                value: Some(format!("{:?}", layout.browser_left_panel_order)),
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.panel.reorder".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "tooltip-sidebar-panel-drag"),
                workspace: ws,
                enabled: layout.panel("browser.sidebar"),
                visible: layout.panel("browser.sidebar"),
                bounds: None,
                value: Some(format!("{:?}", layout.browser_left_panel_order)),
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.right_panel.reorder".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "tooltip-panel-drag-reorder"),
                workspace: ws,
                enabled: layout.panel("browser.preview") && layout.panel("browser.info"),
                visible: layout.panel("browser.preview") && layout.panel("browser.info"),
                bounds: None,
                value: Some(format!("{:?}", layout.browser_right_panel_order)),
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.preview".to_owned(),
                role: "panel".to_owned(),
                label: tr(lang, "panel-preview"),
                workspace: ws,
                enabled: layout.panel("browser.preview"),
                visible: layout.panel("browser.preview"),
                bounds: None,
                value: snapshot
                    .meta
                    .selection_info
                    .as_ref()
                    .map(|info| info.name.clone()),
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.info".to_owned(),
                role: "panel".to_owned(),
                label: tr(lang, "panel-info"),
                workspace: ws,
                enabled: layout.panel("browser.info"),
                visible: layout.panel("browser.info"),
                bounds: None,
                value: snapshot
                    .meta
                    .selection_info
                    .as_ref()
                    .map(|info| info.name.clone()),
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.inspector".to_owned(),
                role: "panel".to_owned(),
                label: tr(lang, "menu-view-detail-panel"),
                workspace: ws,
                enabled: layout.panel("browser.inspector"),
                visible: layout.panel("browser.inspector"),
                bounds: None,
                value: snapshot
                    .meta
                    .selection_info
                    .as_ref()
                    .map(|info| info.name.clone()),
            });
            widgets.push(SemanticWidgetNode {
                id: "browser.grid".to_owned(),
                role: "list".to_owned(),
                label: tr(lang, "workspace-browser"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: Some(format!("{} entries", snapshot.entries.len())),
            });
        }
        Workspace::Viewer => {
            let has_nav = snapshot.meta.viewer_count > 1;
            let info_open = viewer_ui.info_open;
            widgets.push(SemanticWidgetNode {
                id: "viewer.canvas".to_owned(),
                role: "canvas".to_owned(),
                label: tr(lang, "workspace-viewer"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: snapshot.meta.viewer_path.clone(),
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.prev".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-prev"),
                workspace: ws,
                enabled: has_nav,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.next".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-next"),
                workspace: ws,
                enabled: has_nav,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.zoom_out".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-zoom-out"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: Some(format!("{zoom:.2}")),
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.zoom_in".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-zoom-in"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: Some(format!("{zoom:.2}")),
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.fit".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-fit"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: Some(format!("{zoom:.2}")),
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.actual_size".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-actual-ratio"),
                workspace: ws,
                enabled: snapshot.meta.viewer_path.is_some(),
                visible: true,
                bounds: None,
                value: Some(format!("{zoom:.2}")),
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.rotate_left".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-rotate-left"),
                workspace: ws,
                enabled: snapshot.meta.viewer_path.is_some(),
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.rotate_right".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-rotate-right"),
                workspace: ws,
                enabled: snapshot.meta.viewer_path.is_some(),
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.filmstrip_toggle".to_owned(),
                role: "toggle".to_owned(),
                label: tr(lang, "action-filmstrip"),
                workspace: ws,
                enabled: has_nav,
                visible: true,
                bounds: None,
                value: Some(viewer_ui.filmstrip_open.to_string()),
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.fullscreen".to_owned(),
                role: "toggle".to_owned(),
                label: tr(lang, "action-fullscreen"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.edit".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-edit"),
                workspace: ws,
                enabled: snapshot.meta.viewer_path.is_some(),
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.info_toggle".to_owned(),
                role: "toggle".to_owned(),
                label: tr(lang, "action-info"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: Some(info_open.to_string()),
            });
            let vmode = viewer_ui.view_mode;
            widgets.push(SemanticWidgetNode {
                id: "viewer.mode_single".to_owned(),
                role: "toggle".to_owned(),
                label: tr(lang, "action-viewer-single"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: Some((vmode == ViewerViewMode::Single).to_string()),
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.mode_double".to_owned(),
                role: "toggle".to_owned(),
                label: tr(lang, "action-viewer-double"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: Some((vmode == ViewerViewMode::Double).to_string()),
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.mode_manhwa".to_owned(),
                role: "toggle".to_owned(),
                label: tr(lang, "action-viewer-manhwa"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: Some((vmode == ViewerViewMode::Webtoon).to_string()),
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.reader_options".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-reader-options"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            let (viewer_ctx_open, ctx_target_path) = match context_menu {
                Some(ActiveContextMenu::Viewer { target_path, .. }) => (
                    true,
                    target_path
                        .clone()
                        .or_else(|| snapshot.meta.viewer_path.clone()),
                ),
                _ => (false, snapshot.meta.viewer_path.clone()),
            };
            let has_ctx_path = ctx_target_path.is_some();
            let is_webtoon = vmode == ViewerViewMode::Webtoon;
            widgets.push(SemanticWidgetNode {
                id: "viewer.context_menu".to_owned(),
                role: "menu".to_owned(),
                label: tr(lang, "workspace-viewer"),
                workspace: ws,
                enabled: true,
                visible: viewer_ctx_open,
                bounds: None,
                value: ctx_target_path,
            });
            for (suffix, label_key, enabled, value) in [
                ("edit", "action-edit", has_ctx_path, None),
                ("copy_image", "action-copy-image", has_ctx_path, None),
                ("copy_path", "action-copy-path", has_ctx_path, None),
                ("reveal_folder", "action-reveal-folder", has_ctx_path, None),
                ("fit", "action-fit", has_ctx_path, None),
                ("actual_size", "action-actual-size", has_ctx_path, None),
                ("zoom_in", "action-zoom-in", has_ctx_path, None),
                ("zoom_out", "action-zoom-out", has_ctx_path, None),
                (
                    "rotate_left",
                    "action-rotate-left",
                    has_ctx_path && !is_webtoon,
                    None,
                ),
                (
                    "rotate_right",
                    "action-rotate-right",
                    has_ctx_path && !is_webtoon,
                    None,
                ),
                (
                    "mode_single",
                    "action-viewer-single",
                    true,
                    Some((vmode == ViewerViewMode::Single).to_string()),
                ),
                (
                    "mode_double",
                    "action-viewer-double",
                    true,
                    Some((vmode == ViewerViewMode::Double).to_string()),
                ),
                (
                    "mode_manhwa",
                    "action-viewer-manhwa",
                    true,
                    Some((vmode == ViewerViewMode::Webtoon).to_string()),
                ),
                (
                    "fullscreen",
                    "action-fullscreen",
                    true,
                    Some(viewer_ui.is_fullscreen.to_string()),
                ),
                ("info", "action-info", true, Some(info_open.to_string())),
            ] {
                widgets.push(SemanticWidgetNode {
                    id: format!("viewer.context_menu.{suffix}"),
                    role: "menu_item".to_owned(),
                    label: tr(lang, label_key),
                    workspace: ws,
                    enabled,
                    visible: viewer_ctx_open,
                    bounds: None,
                    value,
                });
            }
            let viewer_sidebar_open = info_open || viewer_ui.adjustments_open;
            widgets.push(SemanticWidgetNode {
                id: "viewer.splitter.right".to_owned(),
                role: "splitter".to_owned(),
                label: "Right Splitter".to_owned(),
                workspace: ws,
                enabled: viewer_sidebar_open,
                visible: viewer_sidebar_open,
                bounds: None,
                value: Some(snapshot.meta.layout.right_width.to_string()),
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.panel.reorder".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "tooltip-sidebar-panel-drag"),
                workspace: ws,
                enabled: viewer_sidebar_open,
                visible: viewer_sidebar_open,
                bounds: None,
                value: Some(format!(
                    "{:?}",
                    snapshot.meta.layout.viewer_right_panel_order
                )),
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.adjustments".to_owned(),
                role: "panel".to_owned(),
                label: tr(lang, "panel-view-adjustments"),
                workspace: ws,
                enabled: viewer_ui.adjustments_open,
                visible: viewer_ui.adjustments_open,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "viewer.info".to_owned(),
                role: "panel".to_owned(),
                label: tr(lang, "panel-info"),
                workspace: ws,
                enabled: info_open,
                visible: info_open,
                bounds: None,
                value: None,
            });
            let has_img = snapshot.meta.viewer_path.is_some();
            for (id, role, label_key, enabled, visible, value) in [
                (
                    "viewer.color_popover.toggle",
                    "toggle",
                    "action-viewer-color",
                    has_img && !is_webtoon,
                    true,
                    Some(viewer_ui.color_popover_open.to_string()),
                ),
                (
                    "viewer.color_popover.panel",
                    "panel",
                    "action-viewer-color",
                    viewer_ui.color_popover_open,
                    viewer_ui.color_popover_open,
                    None,
                ),
                (
                    "viewer.color_popover.reset",
                    "button",
                    "action-reset",
                    viewer_ui.color_popover_open,
                    viewer_ui.color_popover_open,
                    None,
                ),
                (
                    "viewer.color_popover.open_editor",
                    "button",
                    "action-open-with-adjustments",
                    viewer_ui.color_popover_open && has_img,
                    viewer_ui.color_popover_open,
                    None,
                ),
            ] {
                widgets.push(SemanticWidgetNode {
                    id: id.to_owned(),
                    role: role.to_owned(),
                    label: tr(lang, label_key),
                    workspace: ws,
                    enabled,
                    visible,
                    bounds: None,
                    value,
                });
            }
        }
        Workspace::Editor => {
            let ed = snapshot.meta.editor.as_ref();
            let has_doc = ed.is_some();
            let can_undo = ed.is_some_and(|e| e.can_undo);
            let can_redo = ed.is_some_and(|e| e.can_redo);
            let modified = ed.is_some_and(|e| e.modified);
            let layer_count = ed.map(|e| e.layers.len()).unwrap_or(0);
            let active_layer_id = ed.map(|e| e.active_layer_id).unwrap_or(0);

            widgets.push(SemanticWidgetNode {
                id: "editor.canvas".to_owned(),
                role: "canvas".to_owned(),
                label: tr(lang, "workspace-editor"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: ed.map(|e| e.name.clone()),
            });

            for (id, label_key, tool) in [
                ("editor.tool.pointer", "tool-pointer", EditorTool::Pointer),
                ("editor.tool.hand", "tool-hand", EditorTool::Hand),
                ("editor.tool.zoom", "tool-zoom", EditorTool::Zoom),
                (
                    "editor.tool.marquee_rect",
                    "tool-marquee-rect",
                    EditorTool::MarqueeRect,
                ),
                (
                    "editor.tool.marquee_ellipse",
                    "tool-marquee-ellipse",
                    EditorTool::MarqueeEllipse,
                ),
                (
                    "editor.tool.single_row",
                    "tool-single-row",
                    EditorTool::SingleRow,
                ),
                (
                    "editor.tool.single_col",
                    "tool-single-col",
                    EditorTool::SingleCol,
                ),
                ("editor.tool.lasso", "tool-lasso", EditorTool::Lasso),
                (
                    "editor.tool.polygon_lasso",
                    "tool-polygon-lasso",
                    EditorTool::PolygonLasso,
                ),
                (
                    "editor.tool.magnetic_lasso",
                    "tool-magnetic-lasso",
                    EditorTool::MagneticLasso,
                ),
                (
                    "editor.tool.bezier_select",
                    "tool-bezier-select",
                    EditorTool::BezierSelect,
                ),
                (
                    "editor.tool.quick_select",
                    "tool-quick-select",
                    EditorTool::QuickSelect,
                ),
                (
                    "editor.tool.magic_wand",
                    "tool-magic-wand",
                    EditorTool::MagicWand,
                ),
                ("editor.tool.brush", "tool-brush", EditorTool::Brush),
                ("editor.tool.eraser", "tool-eraser", EditorTool::Eraser),
                ("editor.tool.liquify", "tool-liquify", EditorTool::Liquify),
                (
                    "editor.tool.eyedropper",
                    "tool-eyedropper",
                    EditorTool::Eyedropper,
                ),
                ("editor.tool.crop", "tool-crop", EditorTool::Crop),
                (
                    "editor.tool.canvas_size",
                    "tool-canvas-size",
                    EditorTool::CanvasSize,
                ),
                (
                    "editor.tool.rotate_angle",
                    "tool-rotate-angle",
                    EditorTool::RotateAngle,
                ),
                (
                    "editor.tool.draw_line",
                    "tool-draw-line",
                    EditorTool::DrawLine,
                ),
                (
                    "editor.tool.draw_rect",
                    "tool-draw-rect",
                    EditorTool::DrawRect,
                ),
                (
                    "editor.tool.draw_ellipse",
                    "tool-draw-ellipse",
                    EditorTool::DrawEllipse,
                ),
                (
                    "editor.tool.draw_polygon",
                    "tool-draw-polygon",
                    EditorTool::DrawPolygon,
                ),
                (
                    "editor.tool.draw_polyline",
                    "tool-draw-polyline",
                    EditorTool::DrawPolyline,
                ),
                (
                    "editor.tool.bezier_pen",
                    "tool-bezier-pen",
                    EditorTool::BezierPen,
                ),
                (
                    "editor.tool.paint_bucket",
                    "tool-paint-bucket",
                    EditorTool::PaintBucket,
                ),
                (
                    "editor.tool.gradient",
                    "tool-gradient",
                    EditorTool::Gradient,
                ),
                (
                    "editor.tool.healing_brush",
                    "tool-healing-brush",
                    EditorTool::HealingBrush,
                ),
                (
                    "editor.tool.transform",
                    "tool-transform",
                    EditorTool::Transform,
                ),
            ] {
                widgets.push(SemanticWidgetNode {
                    id: id.to_owned(),
                    role: "toggle".to_owned(),
                    label: tr(lang, label_key),
                    workspace: ws,
                    enabled: has_doc,
                    visible: true,
                    bounds: None,
                    value: Some((editor_ui.tool == tool).to_string()),
                });
            }

            widgets.push(SemanticWidgetNode {
                id: "editor.rotate_left".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-rotate-left"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.rotate_right".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-rotate-right"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.flip_h".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-flip-h"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.flip_v".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-flip-v"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.crop".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-crop"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.resize".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-resize"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: None,
            });

            let layout = &snapshot.meta.layout;
            let show_editor_inspector = layout.panel("editor.inspector");
            widgets.push(SemanticWidgetNode {
                id: "editor.splitter.right".to_owned(),
                role: "splitter".to_owned(),
                label: "Right Splitter".to_owned(),
                workspace: ws,
                enabled: has_doc && show_editor_inspector,
                visible: has_doc && show_editor_inspector,
                bounds: None,
                value: Some(layout.editor_right_width.to_string()),
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.splitter.stack".to_owned(),
                role: "splitter".to_owned(),
                label: "Stack Splitter".to_owned(),
                workspace: ws,
                enabled: has_doc && show_editor_inspector,
                visible: has_doc && show_editor_inspector,
                bounds: None,
                value: Some(layout.editor_right_split_pct.to_string()),
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.inspector.top".to_owned(),
                role: "panel".to_owned(),
                label: tr(lang, "label-history"),
                workspace: ws,
                enabled: has_doc && show_editor_inspector,
                visible: has_doc && show_editor_inspector,
                bounds: None,
                value: Some(format!("{:?}", layout.editor_tool_tab)),
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.tool_tab.options".to_owned(),
                role: "tab".to_owned(),
                label: tr(lang, "tab-tool-options"),
                workspace: ws,
                enabled: has_doc && show_editor_inspector,
                visible: has_doc && show_editor_inspector,
                bounds: None,
                value: Some(
                    (layout.editor_tool_tab == photon_protocol::EditorToolTab::Options).to_string(),
                ),
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.tool_tab.history".to_owned(),
                role: "tab".to_owned(),
                label: tr(lang, "tab-history"),
                workspace: ws,
                enabled: has_doc && show_editor_inspector,
                visible: has_doc && show_editor_inspector,
                bounds: None,
                value: Some(
                    (layout.editor_tool_tab == photon_protocol::EditorToolTab::History).to_string(),
                ),
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.panel.reorder".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "tooltip-sidebar-panel-drag"),
                workspace: ws,
                enabled: has_doc && show_editor_inspector,
                visible: has_doc && show_editor_inspector,
                bounds: None,
                value: Some(format!("{:?}", layout.editor_right_panel_order)),
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.inspector.layers_bottom".to_owned(),
                role: "panel".to_owned(),
                label: tr(lang, "tab-layers"),
                workspace: ws,
                enabled: has_doc && show_editor_inspector,
                visible: has_doc && show_editor_inspector,
                bounds: None,
                value: Some(format!("{layer_count} layers")),
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.menu.file.open_viewer".to_owned(),
                role: "menu_item".to_owned(),
                label: tr(lang, "menu-file-open-viewer"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.menu.file.open_browser".to_owned(),
                role: "menu_item".to_owned(),
                label: tr(lang, "menu-file-open-browser"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.menu.filter".to_owned(),
                role: "menu_button".to_owned(),
                label: tr(lang, "menu-filter"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.menu.filter.denoise".to_owned(),
                role: "menu_item".to_owned(),
                label: tr(lang, "menu-filter-denoise"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.menu.plugins".to_owned(),
                role: "menu_button".to_owned(),
                label: tr(lang, "menu-plugins"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.menu.plugins.manage".to_owned(),
                role: "menu_item".to_owned(),
                label: tr(lang, "action-manage-plugins"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.menu.plugins.reload".to_owned(),
                role: "menu_item".to_owned(),
                label: tr(lang, "action-reload-plugins"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.zoom_in".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-zoom-in"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: Some(format!("{zoom:.2}")),
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.zoom_out".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-zoom-out"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: Some(format!("{zoom:.2}")),
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.zoom_fit".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-fit"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: Some(format!("{zoom:.2}")),
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.zoom_actual".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-actual-ratio"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: Some(format!("{zoom:.2}")),
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.menu.select".to_owned(),
                role: "menu_button".to_owned(),
                label: tr(lang, "menu-select"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.menu.window".to_owned(),
                role: "menu_button".to_owned(),
                label: tr(lang, "menu-window"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });
            for (id, label_key, checked) in [
                (
                    "editor.menu.window.tools",
                    "panel-tools",
                    layout.panel("editor.tools"),
                ),
                (
                    "editor.menu.window.history",
                    "panel-tool-options-history",
                    layout.panel("editor.history"),
                ),
                (
                    "editor.menu.window.layers",
                    "panel-layers",
                    layout.panel("editor.layers"),
                ),
            ] {
                widgets.push(SemanticWidgetNode {
                    id: id.to_owned(),
                    role: "menu_item".to_owned(),
                    label: tr(lang, label_key),
                    workspace: ws,
                    enabled: true,
                    visible: true,
                    bounds: None,
                    value: Some(checked.to_string()),
                });
            }
            widgets.push(SemanticWidgetNode {
                id: "editor.menu.help".to_owned(),
                role: "menu_button".to_owned(),
                label: tr(lang, "menu-help"),
                workspace: ws,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            });

            let adjust_open = editor_ui.adjust_popup_tab.is_some();
            widgets.push(SemanticWidgetNode {
                id: "editor.adjust_popup.open".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-adjustments"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: Some(adjust_open.to_string()),
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.adjust_popup.panel".to_owned(),
                role: "dialog".to_owned(),
                label: tr(lang, "action-adjustments"),
                workspace: ws,
                enabled: adjust_open,
                visible: adjust_open,
                bounds: None,
                value: editor_ui.adjust_popup_tab.map(|t| format!("{t:?}")),
            });
            for (id, label_key, tab) in [
                (
                    "editor.adjust.color",
                    "dialog-adjust-color",
                    AdjustmentPopupTab::ColorAndLight,
                ),
                (
                    "editor.adjust.hsv",
                    "dialog-adjust-hsv",
                    AdjustmentPopupTab::Hsv,
                ),
                (
                    "editor.adjust.balance",
                    "dialog-adjust-balance",
                    AdjustmentPopupTab::ColorBalance,
                ),
                (
                    "editor.adjust.curves",
                    "dialog-adjust-curves",
                    AdjustmentPopupTab::Curves,
                ),
            ] {
                let open_tab = editor_ui.adjust_popup_tab == Some(tab);
                widgets.push(SemanticWidgetNode {
                    id: id.to_owned(),
                    role: "dialog".to_owned(),
                    label: tr(lang, label_key),
                    workspace: ws,
                    enabled: has_doc,
                    visible: open_tab,
                    bounds: None,
                    value: Some(open_tab.to_string()),
                });
            }
            for (id, label_key) in [
                ("editor.adjust_popup.apply", "action-apply"),
                ("editor.adjust_popup.cancel", "action-cancel"),
                ("editor.adjust_popup.reset", "action-reset"),
            ] {
                widgets.push(SemanticWidgetNode {
                    id: id.to_owned(),
                    role: "button".to_owned(),
                    label: tr(lang, label_key),
                    workspace: ws,
                    enabled: adjust_open,
                    visible: adjust_open,
                    bounds: None,
                    value: None,
                });
            }

            let active_layer = ed.and_then(|e| e.layers.iter().find(|l| l.id == e.active_layer_id));
            let active_has_mask = active_layer.is_some_and(|l| l.has_mask);
            for (id, label_key, enabled) in [
                (
                    "editor.layer.mask.add",
                    "action-mask-add",
                    has_doc && !active_has_mask,
                ),
                (
                    "editor.layer.mask.edit",
                    "action-mask-edit",
                    has_doc && active_has_mask,
                ),
                (
                    "editor.layer.mask.enable",
                    "action-mask-enable",
                    has_doc && active_has_mask,
                ),
                (
                    "editor.layer.mask.invert",
                    "action-mask-invert",
                    has_doc && active_has_mask,
                ),
                (
                    "editor.layer.mask.apply",
                    "action-mask-apply",
                    has_doc && active_has_mask,
                ),
                (
                    "editor.layer.mask.delete",
                    "action-mask-delete",
                    has_doc && active_has_mask,
                ),
            ] {
                widgets.push(SemanticWidgetNode {
                    id: id.to_owned(),
                    role: "button".to_owned(),
                    label: tr(lang, label_key),
                    workspace: ws,
                    enabled,
                    visible: true,
                    bounds: None,
                    value: None,
                });
            }

            widgets.push(SemanticWidgetNode {
                id: "editor.layers".to_owned(),
                role: "list".to_owned(),
                label: tr(lang, "tab-layers"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: Some(format!("{layer_count} layers (active {active_layer_id})")),
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.layer.add".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-layer-add"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.layer.duplicate".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-layer-duplicate"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.layer.delete".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-layer-delete"),
                workspace: ws,
                enabled: has_doc && layer_count > 1,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.layer.merge_down".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-layer-merge-down"),
                workspace: ws,
                enabled: has_doc && layer_count > 1,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.layer.flatten".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-layer-flatten"),
                workspace: ws,
                enabled: has_doc && layer_count > 1,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.compare".to_owned(),
                role: "toggle".to_owned(),
                label: tr(lang, "action-compare-original"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: Some(format!("{:?}", editor_ui.compare_mode)),
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.undo".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-undo"),
                workspace: ws,
                enabled: can_undo,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.redo".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-redo"),
                workspace: ws,
                enabled: can_redo,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.reset".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-reset"),
                workspace: ws,
                enabled: modified,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.export".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-export"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: None,
            });
            widgets.push(SemanticWidgetNode {
                id: "editor.save".to_owned(),
                role: "button".to_owned(),
                label: tr(lang, "action-save"),
                workspace: ws,
                enabled: has_doc,
                visible: true,
                bounds: None,
                value: None,
            });

            let editor_ctx_open =
                matches!(context_menu, Some(ActiveContextMenu::EditorCanvas { .. }));
            widgets.push(SemanticWidgetNode {
                id: "editor.context_menu".to_owned(),
                role: "menu".to_owned(),
                label: tr(lang, "workspace-editor"),
                workspace: ws,
                enabled: has_doc,
                visible: editor_ctx_open,
                bounds: None,
                value: None,
            });
            for (suffix, label_key, enabled) in [
                ("undo", "action-undo", can_undo),
                ("redo", "action-redo", can_redo),
                ("copy_image", "action-copy-image", has_doc),
                (
                    "duplicate_layer",
                    "action-layer-duplicate",
                    ed.is_some_and(|e| !e.layers.is_empty()),
                ),
                ("rotate_left", "action-rotate-left", has_doc),
                ("rotate_right", "action-rotate-right", has_doc),
                ("flip_h", "action-flip-h", has_doc),
                ("flip_v", "action-flip-v", has_doc),
                ("crop", "action-crop", has_doc),
                ("resize", "action-resize", has_doc),
                ("canvas_size", "tool-canvas-size", has_doc),
                ("fit", "action-fit", has_doc),
                ("compare", "action-compare-original", has_doc),
                (
                    "compare_side_by_side",
                    "label-compare-side-by-side",
                    has_doc,
                ),
                ("reset", "action-reset", modified),
            ] {
                widgets.push(SemanticWidgetNode {
                    id: format!("editor.context_menu.{suffix}"),
                    role: "menu_item".to_owned(),
                    label: tr(lang, label_key),
                    workspace: ws,
                    enabled,
                    visible: editor_ctx_open,
                    bounds: None,
                    value: match suffix {
                        "compare" => Some((editor_ui.compare_mode != CompareMode::Off).to_string()),
                        "compare_side_by_side" => {
                            Some((editor_ui.compare_mode == CompareMode::SideBySide).to_string())
                        }
                        _ => None,
                    },
                });
            }

            let (layer_ctx_open, target_layer_id) = match context_menu {
                Some(ActiveContextMenu::EditorLayer { layer_id, .. }) => (true, *layer_id),
                _ => (false, active_layer_id),
            };
            let target_idx = ed.and_then(|e| e.layers.iter().position(|l| l.id == target_layer_id));
            let target_layer = ed.and_then(|e| target_idx.map(|i| &e.layers[i]));
            let can_move_up = target_idx
                .zip(ed)
                .is_some_and(|(i, e)| i + 1 < e.layers.len());
            let can_move_down = target_idx.is_some_and(|i| i > 0);
            let can_merge_down = target_idx
                .zip(ed)
                .is_some_and(|(i, e)| i > 0 && !e.layers[i].locked && !e.layers[i - 1].locked);
            let can_flatten = layer_count > 1;
            let can_delete = layer_count > 1 && target_layer.is_some_and(|l| !l.locked);

            widgets.push(SemanticWidgetNode {
                id: "editor.layer.context_menu".to_owned(),
                role: "menu".to_owned(),
                label: tr(lang, "tab-layers"),
                workspace: ws,
                enabled: target_layer.is_some(),
                visible: layer_ctx_open,
                bounds: None,
                value: target_layer.map(|l| l.id.to_string()),
            });
            for (suffix, label_key, enabled) in [
                (
                    "duplicate",
                    "action-layer-duplicate",
                    target_layer.is_some(),
                ),
                ("rename", "action-rename", target_layer.is_some()),
                ("move_up", "action-layer-move-up", can_move_up),
                ("move_down", "action-layer-move-down", can_move_down),
                ("merge_down", "action-layer-merge-down", can_merge_down),
                ("flatten", "action-layer-flatten", can_flatten),
                (
                    "toggle_visible",
                    if target_layer.is_some_and(|l| l.visible) {
                        "action-layer-hide"
                    } else {
                        "action-layer-show"
                    },
                    target_layer.is_some(),
                ),
                (
                    "toggle_locked",
                    if target_layer.is_some_and(|l| l.locked) {
                        "action-layer-unlock"
                    } else {
                        "action-layer-lock"
                    },
                    target_layer.is_some(),
                ),
                ("delete", "action-layer-delete", can_delete),
            ] {
                widgets.push(SemanticWidgetNode {
                    id: format!("editor.layer.context_menu.{suffix}"),
                    role: "menu_item".to_owned(),
                    label: tr(lang, label_key),
                    workspace: ws,
                    enabled,
                    visible: layer_ctx_open,
                    bounds: None,
                    value: None,
                });
            }
        }
    }

    let gpu_adapter = match gpu_backend {
        "tiny-skia" => "tiny-skia (software)".to_owned(),
        "headless" => "headless".to_owned(),
        _ => "iced_wgpu".to_owned(),
    };

    UiTreeSnapshot {
        revision: snapshot.meta.revision,
        workspace: ws,
        presentation: PresentationState {
            window_count,
            window_width: window_size.width,
            window_height: window_size.height,
            scale_factor: 1.0,
            zoom,
            pan_x,
            pan_y,
            gpu_backend: gpu_backend.to_owned(),
            gpu_adapter,
        },
        widgets,
    }
}
