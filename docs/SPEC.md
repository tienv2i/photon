# Photon Functional & UI Specification

This document specifies the implemented functional and UI behavior across `photon-dialogs`, `photon-panels`, `photon-ui`, `photon-canvas`, `photon-plugins`, `photon-core`, `photon-browser`, `photon-viewer`, `photon-editor`, and `photon-desktop`.

## 1. Typography & Design Tokens (`photon-ui`)

- `PaletteTokens::DARK` and `PaletteTokens::LIGHT` provide neutral studio surfaces (`bg_app`, `bg_canvas`, `bg_surface`, `bg_elevated`, `border`, `accent`).
- Standard font sizes (`crates/photon-ui/src/theme.rs`):
  - `TEXT_SIZE_DIALOG_TITLE` = `14.0` px (dialog & brand titles)
  - `TEXT_SIZE_CONTROL` = `12.0` px (controls, body copy, `checkbox`, `text_input`, `pick_list`, menu items, status bar)
  - `TEXT_SIZE_CAPTION` = `11.0` px (captions, shortcuts, badges)
- `crates/photon-ui/src/curve_editor.rs` provides `CurveGraphProgram` (`curve_graph`), a `256×256` interactive monotone cubic Hermite spline editor with 4×4 grid, diagonal identity reference, optional luminance histogram backdrop, drag-clamped control points, right-click/double-click interior point removal, and `(In, Out)` `0..255` readout.

## 2. Unified Dialog System (`photon-dialogs` + `photon_ui::dialog`)

- **Native OS pickers (`crates/photon-dialogs/src/system.rs`)**: `rfd::AsyncFileDialog` wrappers (`pick_folder`, `open_file`, `open_files`, `open_images`, `save_file`, `save_image_as`) using `IMAGE_EXTENSIONS` (including `.ora`, `.psd`, `.jxl`, `.svg`/`.svgz`, `.avif`, `.exr`, `.hdr`, `.ico`, `.cur`, `.pam`, `.dds`, `.ff`, `.apng`, `.heic`/`.heif`, and Camera RAW `.dng`/`.cr2`/`.cr3`/`.nef`/`.arw`/`.raf`/`.orf`/`.rw2`).
- **Managed custom dialogs (`crates/photon-dialogs/src/manager.rs`, `frame.rs`)**:
  - `DialogManager<DesktopDialogId>` tracks per-dialog `DialogWindowState`, z-order, and active `DragOp::{Move, Resize(ResizeEdge)}`.
  - **Initial placement & auto-fit**: When `x`/`y` are `None` (`!user_moved && !user_resized`), dialogs center inside the viewport with `16.0` px margin (`DialogWindowState::margin`) and size vertically to content (`Length::Shrink`, capped at `viewport.height - 2 * margin`).
  - **Draggable titlebar**: `TitleDragProgram` (`canvas::Program`) drags the dialog within clamped viewport bounds; double-clicking within `350` ms emits `DialogEvent::Center` to re-center.
  - **8-way resizing**: `ResizeEdge::{N, S, E, W, NE, NW, SE, SW}` handles pin the opposite edge and clamp between `(min_width, min_height)` and `(max_width, max_height)` within viewport margins. Once resized (`user_resized`), the frame uses fixed `(rect.width, rect.height)` with `Length::Fill` scrollable body.
  - **Overlay modes (`crates/photon-dialogs/src/overlay.rs`)**:
    - `OverlayMode::DimmedFrosted { dim_alpha, dismiss_on_backdrop }`: full-viewport tinted scrim (`dim_alpha = 0.56` default) wrapped in `opaque(mouse_area(...))` to block underlying interaction.
    - `OverlayMode::Cutout { cutout, dim_alpha, lock_outside }`: partitions `viewport \ cutout` via `compute_cutout_strips` into 4 non-overlapping `opaque` perimeter strips (`top`, `bottom`, `left`, `right`) while leaving the `cutout` region completely undimmed and pass-through so the live GPU canvas remains interactive.
    - `OverlayMode::Floating`: no scrim; only the dialog card itself is `opaque`.
  - `managed_dialog_stack` layers active dialogs (`Settings`, `ManagePlugins`, `PluginParams`, `InAppDialog`, `Batch*`, `SelectionModify`, or the Editor adjustment dialogs `AdjustColor`, `AdjustHsv`, `AdjustColorBalance`, `AdjustCurves`, `AdjustDenoise`) over the workspace and toast banner.
  - `DesktopDialogId::AdjustCurves` defaults to `460×520` (`min_size(380, 440)`); `DesktopDialogId::ManagePlugins` defaults to `640×480`.

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

### Browser (`crates/photon-browser/src/lib.rs`)
- **Launch & Window Settings**: When launching Photon directly into Browser (`!opened_with_files`), `LayoutState.app_launch_window_mode` (`AppLaunchWindowMode::{Maximized, Remembered}`, defaulting to `Remembered` with first-run `window_maximized: true`) maximizes or restores the previous window state. Settings separates **App Startup Window (Browser)** from **Opening Image Files** (`SystemOpenWindowMode`: `Fit Image` | `Fullscreen` | `Remembered`), and exposes **Hardware Acceleration** (showing active `GPU (wgpu)` vs `Software (tiny-skia)` + restart-required notice), **Canvas Anti-Aliasing** (`Crisp` / `Smooth` / `HighQuality`), and **Viewport Denoise** (`Off` / `Light` / `Medium` / `Strong`).
- **Toolbar**: `[icon] Photon`, menubar (`File`, `Edit`, `View`, `Tools`, `Plugins`, `Window`, `Help`) with hover-to-switch while a menu is open, right-aligned `workspace_switcher_compact` (`Browser | Viewer | Editor`), Theme toggle, and Settings button.
- **Tools menu**: Separate entries for **Batch Resize…** (`BatchDialogKind::Resize`), **Batch Crop…** (`BatchDialogKind::Crop`), and **Batch Convert…** (`BatchDialogKind::Convert`), plus **Open in Editor**. Batch dialogs (`batch_dialog.rs`) support selection vs current folder, beside-originals or chosen output folder, filename suffix, overwrite/duplicate guardrails, `px`/`cm`/`in`/`%` + DPI units (`BatchResize`), and anchor/ratio/unit (`BatchCrop`).
- **Plugins menu**: Lists enabled `PluginCapability::BrowserAction` plugins alongside **Manage Plugins…** (`DesktopDialogId::ManagePlugins`) and **Reload Plugins**.
- **Copy / Paste**: `Ctrl+C` and `Ctrl+V` (plus toolbar / context menu actions) copy selected files or paste clipboard image bytes (`Command::PasteImageBytes`) into the current folder with automatic collision renaming (`stem (copy).ext`).
- **Content & context menus**: Path breadcrumbs + editable path bar, live filter input, sort menu (`Name`, `Modified`, `Size`, `Kind`, `Asc`/`Desc`), `XS`–`XL` thumbnail stepper, `Grid` | `List` | `Details` modes, live batch progress banner, and cursor-positioned context menus (`BrowserEntry`, `BrowserQuickAccess`) using global `last_cursor` coordinates.
- **Detail panel**: Manual `Info`/`Detail` right dock (`browser.inspector`) showing thumbnail preview, metadata (`kv_row`), and workspace/folder actions without auto-opening on selection.

### Viewer (`crates/photon-viewer/src/lib.rs`)
- **Viewport**: Clean GPU `Shader` / multi-pane / scroll viewport with **no floating canvas HUD**; bottom `filmstrip_bar` (`viewer.filmstrip`) defaults to `false`.
- **Toolbar**: Left-aligned `[icon] Photon`, Prev/counter/Next, `Single | Double | Webtoon` mode switcher, `Reader Options` popover (`LTR`/`RTL`, separate cover, `0`/`8`/`16` px page gap, Webtoon fit `Auto`/`Width`/`Height`/`Original`, plus **Canvas Anti-Aliasing** & **Viewport Denoise**), Slideshow toggle, Zoom Out / `%` / Zoom In / `Fit` / `1:1`, Rotate Left/Right, `View Adjustment` toggle, `Filmstrip` toggle, icon-only `Info` toggle, Fullscreen toggle, and responsive `More` overflow menu (`ViewerOverflow`). Right-aligned `workspace_switcher_compact` + Theme + Settings (utility cluster only in auxiliary windows).
- **Right dock**: Unified `sidebar_dock` holding `View Adjustments` (7 real-time GPU sliders: Exposure, Gamma, Highlights, Shadows, Brightness, Contrast, Saturation + Reset + **Open in Editor** transfer) and `Info` (Format, Dimensions, File Size, Path + Copy Path).

### Editor (`crates/photon-editor/src/lib.rs`)
- **Viewer (`crates/photon-viewer/src/lib.rs`)**: Independent Viewer windows (`StandaloneViewer` via `--viewer-window`) + clean GPU `Shader` / multi-pane / scroll viewport supporting custom **Canvas Backdrop** (`viewer_viewport_background`), optional bottom `filmstrip_bar`, and unified `View Adjustments` + `Info` right dock.
- **Editor (`crates/photon-editor/src/lib.rs`)**: Multi-tab document bar below the toolbar (`document_tab_bar`, `Ctrl+Tab`, `Alt+1..9`, `Ctrl+W` dirty prompt), menu-driven **Image Size…** (`Ctrl+R`) and **Canvas Size…** (`Ctrl+Shift+R`) popups (removed from the left tool rail), multi-stop interactive **Gradient** (`G`, 6 shapes, 3 repeat modes, 7 blend modes, 8 presets), selection-clipped `Brush`/`Eraser`, and selection-aware `Free Transform` (`Ctrl+T`).
- **Workspace & View Menu + 5-Tab Settings**: Auto-closing toolbar menu (`Workspace & View`) with theme toggle, panel toggles, Viewer Canvas Backdrop options, **Save Current UI Layout**, **Reset Layout**, and **Settings** (`Ctrl+,`). Settings has 5 tabs (`General`, `Viewer`, `Quick Access`, `Plugins` with `PluginModels` & `TranslateSettings` modals, `Translate`), `save_ui_state_on_exit` toggle, and Safe Mode **Apply Safe Defaults to Disk**.
- **File menu**: Includes `Open in Viewer` (`Esc`, leaves document open in `Core` if clean or prompts `ConfirmUnsavedLeave`), `Open in Browser`, `Close`, `Save` (preserving multi-layer `.ora` stacks with `.bak` backup), `Save As…`, `Export…` (`SaveFormat::CHOICES`), and `Settings`.
- **Edit & Clipboard**: Selection- and layer-aware `Cut` (`Ctrl+X`), `Copy` (`Ctrl+C`), `Copy Merged` (`Ctrl+Shift+C`), `Paste` (`Ctrl+V`), and `Paste Into Selection` backed by `EditorPixelClipboard` + `arboard` (`wl-copy`/`wl-paste`/`xclip` fallback) + `Query::EditorClipboardPng`, plus `Free Transform` (`Ctrl+T`).
- **Status bar & Canvas Navigation**:
  - Displays active document filename, dirty marker (`*`), pixel dimensions (`<name>[*] · <W>×<H>`), and bottom **Zoom `-` / `%` / `+` / `Fit (0)` / `1:1 (1)`** controls.
  - Draggable horizontal/vertical `CanvasScrollbarProgram` overlays appear when displayed size exceeds viewport size (`disp > vp`).
  - **Tool-safe Free Panning**: `Pointer` (when no crop/transform handle is dragged) and `Hand` (`H`) pan on Left-drag without rigid viewport lock; Middle-mouse drag and global `Space + Left-drag` pan freely on all mouse-active tools; `Window::Unfocused` clears transient interaction flags; dynamic zoom range (`0.02 ..= (32.0 / fit_scale).max(32.0)`) guarantees `1:1` pixel inspection even on `32,768px` documents.
- **Left tool rail**:
  - Navigation & inspect: `Pointer`, `Hand` (`H`), `Zoom`, `Eyedropper`, `Transform` (`Ctrl+T`).
  - Selection suite: Marquee (`M`), Lasso / Polygon / Magnetic / Bézier Select, Magic Wand (`W`) / Quick Select / Color Range, Quick Mask (`Q`).
  - Paint, fill & healing: `Brush` / `Eraser` (with `BrushSmoothingMode`), `Liquify`, **3 separate 1-click tools** (**Paint Bucket** `F`, **Gradient** `G`, **Healing Brush** `J`), `Shape` (`U`), `Bézier Pen` (`P`), `Crop`, `Resize`, `Canvas Size`, `Rotate Angle`.
  - Quick 90° rotate & H/V flip; FG/BG `color_pair_widget`; and **Viewport Background** `checker_swatch` (`ColorPickerTarget::ViewportBackground`).
- **Filter menu & Cutout Adjustment dialogs**:
  - `Filter` opens live-preview `OverlayMode::Cutout` dialogs over `resolved_editor_canvas_bounds(state)` (`AdjustColor`, `AdjustHsv`, `AdjustColorBalance`, `AdjustCurves`, `AdjustDenoise`).
  - `AdjustCurves` embeds the interactive `CurveGraphProgram` (`256×256` curve + luminance histogram + channel reset + 5 presets) and drives a live `256×1` RGBA8 GPU 1D-LUT on the canvas.
  - `AdjustDenoise` (`Command::EditorDenoise` / `LayerEffect::Denoise`) applies selection-aware bilateral denoising (`denoise_rgba`) that weights taps by `SelectionMask` alpha to prevent exterior border bleed.
- **Free Transform (`Ctrl+T`)**:
  - `TransformPreview` composites `base_frame` (excluding active layer) and `layer_frame` through an inverse 2D affine matrix in `WGSL_SHADER` at 60fps while dragging OBB Move, 8-way Scale (`Shift` aspect-lock, `Alt` center-scale), Rotate, or Pivot handles, and commits once on `Enter` / `Apply` (`Command::EditorTransformLayer`) or discards on `Esc` / `Cancel`.
- **Plugins menu**: Lists enabled `PluginCapability::EditorFilter` plugins alongside `Manage Plugins…` and `Reload Plugins`.
- **Layers panel**: KogaView-style `Layers` section with blend mode `pick_list`, opacity slider, single-row mask controls (add reveal/hide, edit mask/pixels, enable/disable, link/unlink, invert, apply, delete), `layer_action_bar`, `layer_row` list (inline rename, visibility, lock, right-click context menu), filter layer controls (including `LayerEffect::Denoise`), and dedicated `Canvas Background` row at the base.

## 5. Plugin Bridge & Legacy AI/Translate Compatibility

- **Plugins Enabled (`photon-plugins`)**: External plugins run in isolated child processes over NDJSON (`PhotonNdjsonV1`) or KogaView `jsonl-stdio` (`JsonlStdioV1`), managed through **Plugins → Manage Plugins…** or `Command::{PluginRefresh, PluginSetEnabled, PluginInstall, PluginUninstall, PluginRun}` (see [PLUGINS.md](PLUGINS.md)).
- **Legacy Built-in Translation Commands**: Protocol commands (`Command::ViewerTranslate`, `ViewerSaveTranslation`, `ViewerDeleteTranslation`, `SetTranslateSettings`) and `Config.translate` (`TranslateSettings`) are retained for serde backward compatibility and deterministic `commands.jsonl` replay. `Core::dispatch` rejects those legacy built-in variants with `ProtocolError::line("feature_disabled", UiText::new("ai_feature_disabled"))`; OCR and translation features are provided by external plugins (`photon.translate`, `photon.ocr.tesseract`, `photon.ocr.ai`).
