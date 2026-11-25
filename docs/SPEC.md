# Photon Functional & UI Specification

This document specifies the implemented functional and UI behavior across `photon-dialogs`, `photon-panels`, `photon-ui`, `photon-core`, and `photon-desktop`.

## 1. Typography & Design Tokens (`photon-ui`)

- `PaletteTokens::DARK` and `PaletteTokens::LIGHT` provide neutral studio surfaces (`bg_app`, `bg_canvas`, `bg_surface`, `bg_elevated`, `border`, `accent`).
- Standard font sizes (`crates/photon-ui/src/theme.rs`):
  - `TEXT_SIZE_DIALOG_TITLE` = `14.0` px (dialog & brand titles)
  - `TEXT_SIZE_CONTROL` = `12.0` px (controls, body copy, `checkbox`, `text_input`, `pick_list`, menu items, status bar)
  - `TEXT_SIZE_CAPTION` = `11.0` px (captions, shortcuts, badges)

## 2. Unified Dialog System (`photon-dialogs` + `photon_ui::dialog`)

- **Native OS pickers (`crates/photon-dialogs/src/system.rs`)**: `rfd::AsyncFileDialog` wrappers (`pick_folder`, `open_file`, `open_files`, `open_images`, `save_file`, `save_image_as`) using `IMAGE_EXTENSIONS`.
- **Managed custom dialogs (`crates/photon-dialogs/src/manager.rs`, `frame.rs`)**:
  - `DialogManager<DesktopDialogId>` tracks per-dialog `DialogWindowState`, z-order, and active `DragOp::{Move, Resize(ResizeEdge)}`.
  - **Initial placement & auto-fit**: When `x`/`y` are `None` (`!user_moved && !user_resized`), dialogs center inside the viewport with `16.0` px margin (`DialogWindowState::margin`) and size vertically to content (`Length::Shrink`, capped at `viewport.height - 2 * margin`).
  - **Draggable titlebar**: `TitleDragProgram` (`canvas::Program`) drags the dialog within clamped viewport bounds; double-clicking within `350` ms emits `DialogEvent::Center` to re-center.
  - **8-way resizing**: `ResizeEdge::{N, S, E, W, NE, NW, SE, SW}` handles pin the opposite edge and clamp between `(min_width, min_height)` and `(max_width, max_height)` within viewport margins. Once resized (`user_resized`), the frame uses fixed `(rect.width, rect.height)` with `Length::Fill` scrollable body.
  - **Overlay modes (`crates/photon-dialogs/src/overlay.rs`)**:
    - `OverlayMode::DimmedFrosted { dim_alpha, dismiss_on_backdrop }`: full-viewport tinted scrim (`dim_alpha = 0.56` default) wrapped in `opaque(mouse_area(...))` to block underlying interaction.
    - `OverlayMode::Cutout { cutout, dim_alpha, lock_outside }`: partitions `viewport \ cutout` via `compute_cutout_strips` into 4 non-overlapping `opaque` perimeter strips (`top`, `bottom`, `left`, `right`) while leaving the `cutout` region completely undimmed and pass-through so the live GPU canvas remains interactive.
    - `OverlayMode::Floating`: no scrim; only the dialog card itself is `opaque`.
  - `managed_dialog_stack` layers active dialogs (`Settings`, `InAppDialog`, or one of the 4 Editor adjustment dialogs) over the workspace and toast banner.

## 3. Unified Dockable Panels (`photon-panels` + `photon-protocol/src/layout.rs`)

- **Ordering & DnD (`crates/photon-panels/src/order.rs`)**: `SidebarOrderState<P>` normalizes persisted order lists (`normalize_panel_order`), tracks `dragging` and `drop_target`, computes `DropIndicator::{Before, After, None}`, and supports both drag-and-drop (`begin_drag`, `hover_drop`, `commit_drop`) and explicit swap (`swap_pair`).
- **Headers (`crates/photon-panels/src/header.rs`)**: `DraggablePanelHeaderSpec` renders `[grip] [icon] title [badge] [swap] [chevron] [close X]`. `DraggablePanelHeaderSpec::with_tabs` replaces the static title with a single-row inline tab strip (`inline_panel_tab_bar`) in the same header bar.
- **Collapse vs `Window`-menu visibility**:
  - Collapse (`chevron` button) toggles local collapsed state without hiding the panel entry.
  - `Window` menu (and panel `close X` buttons in Viewer) toggles persisted `LayoutState.panels` visibility (`browser.quick_access`, `browser.explorer`, `browser.inspector`, `viewer.adjustments`, `viewer.info`, `editor.tools`, `editor.history`, `editor.layers`).
- **Per-workspace docks**:
  - **Browser**: Left dock (`left_width`, default `240` px) stacks `BrowserLeftPanel::{QuickAccess, Explorer}` (`browser_left_panel_order`); optional right dock (`right_width`, default `290` px) shows `Detail/Info` (`browser.inspector`, default `false`, manual toggle only).
  - **Viewer**: Unified right dock (`right_width`, default `290` px) stacks `ViewerRightPanel::{ViewAdjustment, Info}` (`viewer_right_panel_order`, both default `false`) separated by `vertical_panel_splitter` when both are open.
  - **Editor**: Left `Tools` dock (`96` px, `editor.tools`) + right dock (`editor_right_width`, default `300` px) stacking `EditorRightPanel::{ToolOptionsHistory, Layers}` (`editor_right_panel_order`, split via `editor_right_split_pct` default `58%`) with inline tabs `EditorToolTab::{Options, History}` in `ToolOptionsHistory`.

## 4. Workspaces

### Browser (`crates/photon-desktop/src/browser_view.rs`)
- **Toolbar**: `[icon] Photon`, menubar (`File`, `View`, `Tools`, `Plugins`, `Window`, `Help`) with hover-to-switch while a menu is open, right-aligned `workspace_switcher_compact` (`Browser | Viewer | Editor`), Theme toggle, and Settings button.
- **Tools menu**: Separate entries for **Batch Resize…** (`BatchDialogKind::Resize`), **Batch Crop…** (`BatchDialogKind::Crop`), and **Batch Convert…** (`BatchDialogKind::Convert`), plus **Open in Editor**. Batch dialogs (`batch_dialog.rs`) support selection vs current folder, beside-originals or chosen output folder, filename suffix, overwrite/duplicate guardrails, `px`/`cm`/`in`/`%` + DPI units (`BatchResize`), and anchor/ratio/unit (`BatchCrop`).
- **Plugins menu**: Disabled placeholder menu (`ActiveMenu::BrowserPlugins`) showing non-interactive disabled notices for AI and Plugins.
- **Content & context menus**: Path breadcrumbs + editable path bar, live filter input, sort menu (`Name`, `Modified`, `Size`, `Kind`, `Asc`/`Desc`), `XS`–`XL` thumbnail stepper, `Grid` | `List` | `Details` modes, live batch progress banner, and cursor-positioned context menus (`BrowserEntry`, `BrowserQuickAccess`) using global `last_cursor` coordinates.
- **Detail panel**: Manual `Info`/`Detail` right dock (`browser.inspector`) showing thumbnail preview, metadata (`kv_row`), and workspace/folder actions without auto-opening on selection.

### Viewer (`crates/photon-desktop/src/viewer_view.rs`)
- **Viewport**: Clean GPU `Shader` / multi-pane / scroll viewport with **no floating canvas HUD**; bottom `filmstrip_bar` (`viewer.filmstrip`) defaults to `false`.
- **Toolbar**: Left-aligned `[icon] Photon`, Prev/counter/Next, `Single | Double | Webtoon` mode switcher, `Reader Options` popover (`LTR`/`RTL`, separate cover, `0`/`8`/`16` px page gap, Webtoon fit `Auto`/`Width`/`Height`/`Original`), Slideshow toggle, Zoom Out / `%` / Zoom In / `Fit` / `1:1`, Rotate Left/Right, `View Adjustment` toggle, `Filmstrip` toggle, icon-only `Info` toggle, Fullscreen toggle, and responsive `More` overflow menu (`ViewerOverflow`). Right-aligned `workspace_switcher_compact` + Theme + Settings (utility cluster only in auxiliary windows).
- **Right dock**: Unified `sidebar_dock` holding `View Adjustments` (7 real-time GPU sliders: Exposure, Gamma, Highlights, Shadows, Brightness, Contrast, Saturation + Reset + **Open in Editor** transfer) and `Info` (Format, Dimensions, File Size, Path + Copy Path).

### Editor (`crates/photon-desktop/src/editor_view.rs`)
- **Toolbar**: `[icon] Photon`, menubar (`File`, `Edit`, `Image`, `Filter`, `View`, `Window`, `Help`), Quick Actions (`Undo`, `Redo`, `Compare` cycle `Off`/`Split`/`HoldOriginal`, `Export`, `Save`), right-aligned `workspace_switcher_compact` + Theme + Settings.
- **File menu**: Includes `Open in Viewer` (`Esc`, leaves document open in `Core` if clean or prompts `ConfirmUnsavedLeave`), `Open in Browser`, `Close`, `Save`, `Save As…`, `Export…`, and `Settings`.
- **Status bar**: Displays active document filename, dirty marker (`*`), and pixel dimensions (`<name>[*] · <W>×<H>`).
- **Left tool rail**: `Pointer`, `Hand`, `Zoom`, `Eyedropper`, `Brush`, `Eraser`, `Liquify`, `Crop`, `Resize`, `Canvas Size`, `Rotate Angle`; quick 90° rotate & H/V flip; FG/BG `color_pair_widget`; and **Viewport Background** `checker_swatch` (`ColorPickerTarget::ViewportBackground`).
- **Filter menu & 4 Cutout Adjustment dialogs**: `Filter` opens separate live-preview `OverlayMode::Cutout` dialogs over `resolved_editor_canvas_bounds(state)` (`DesktopDialogId::{AdjustColor, AdjustHsv, AdjustColorBalance, AdjustCurves}`), keeping the canvas interactive (`CanvasTool::Hand`) for pan/zoom while adjusting parameters and targeting `ActiveLayer` / `WholeImage` / `NewLayer`.
- **Layers panel**: KogaView-style `Layers` section with blend mode `pick_list`, opacity slider, single-row mask controls (add reveal/hide, edit mask/pixels, enable/disable, link/unlink, invert, apply, delete), `layer_action_bar`, `layer_row` list (inline rename, visibility, lock, right-click context menu), and dedicated `Canvas Background` row at the base.

## 5. Disabled AI/Translate & Plugins Policy

- AI translation and plugin execution are intentionally disabled in both Core and UI.
- Protocol commands (`Command::ViewerTranslate`, `ViewerSaveTranslation`, `ViewerDeleteTranslation`, `SetTranslateSettings`) and `Config.translate` (`TranslateSettings`) are retained for serde backward compatibility and deterministic `commands.jsonl` replay.
- `Core::dispatch` rejects translation commands with `ProtocolError::line("feature_disabled", UiText::new("ai_feature_disabled"))`, and Browser's `Plugins` menu renders disabled placeholder items.
