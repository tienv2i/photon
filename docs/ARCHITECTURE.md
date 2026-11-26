# Photon Architecture

## Invariant Flow
`iced/wgpu GUI | photonctl CLI | Local IPC | MCP | Replay -> Envelope -> CoreRuntime (Tokio Actor) -> Core Handlers -> AppState + Storage -> watch/broadcast -> Adapters`

## Workspace Crates (16)
| Crate | Responsibility | GUI/GPU Free? |
|---|---|---|
| `photon-protocol` | `Envelope`, `Command`, `Query`, `Event`, `CommandResponse`, `UiText`, `LayoutState`, `PluginManifest`, `LayerTransformSpec`, JSON Schema (`schemars`) | Yes |
| `photon-storage` | `AppIdentity` (`Dev`/`Prod`/`Sandbox`), `AppPaths` (XDG), atomic `Config`, `Journal` (`commands.jsonl`), `tracing` daily JSONL logs | Yes |
| `photon-i18n` | Embedded Fluent `.ftl` (`en-US`, `vi-VN`) + `UiText` translator + key/placeholder parity test | Yes |
| `photon-image` | Image probing, RGBA decode, thumbnails, `.ora`/`.psd`/RAW/SVG/JXL codecs, `EditorDocument`, layers/masks/`SelectionMask`, `healing`, `drawing`, `denoise_rgba`, affine transform, atomic `file~` backup save | Yes |
| `photon-core` | Authoritative `Core` state machine, `CoreRuntime` (Tokio `mpsc` + `watch` + `broadcast`), pixel clipboard, plugin dispatch, batch runner, sandbox `replay` engine | Yes |
| `photon-plugins` | Process-isolated NDJSON (`PhotonNdjsonV1`) & `jsonl-stdio` (`JsonlStdioV1`) plugin discovery, path-traversal-safe validation, and execution bridge | Yes |
| `photon-ipc` | Secure local IPC (`$XDG_RUNTIME_DIR/photon[-dev]/control.sock` `0600` on Unix) with NDJSON request/response & event stream | Yes |
| `photon-dialogs` | Native OS file/folder/save dialogs (`rfd`) + movable/8-way resizable `DialogManager` frames and `DimmedFrosted` / `Cutout` / `Floating` overlays | UI Kit only (no Core dep) |
| `photon-ui` | Reusable pure-`iced` Design System & UI Kit (`theme`, SVG `icons`, `curve_editor`, `button`, `menu`, `dialog` theme bridge, `toolbar`, `grid`, `table`, `editor`, `splitter`, `tooltip`) | UI Kit only (no Core dep) |
| `photon-panels` | Unified dockable, collapsible, closable, drag-reorderable sidebar panel system (`SidebarOrderState`, `DraggablePanelHeaderSpec`, `PanelFrameSpec`, `PanelSplitter`) | UI Kit only (no Core dep) |
| `photon-canvas` | `wgpu 27` `ImageCanvasProgram`, Stage 1/Stage 2 WGSL pipeline, GPU vector overlay, Sobel+Dijkstra `magnetic_snap`, `HealingBrush` & live `TransformPreview` | `iced` + `wgpu` |
| `photon-browser` | Browser workspace UI, file grid/list/details views, clipboard/plugins menus, and batch resize/crop/convert dialogs | `iced` UI |
| `photon-viewer` | Viewer workspace UI, spread/webtoon layout, filmstrip, quality options, and view adjustments dock | `iced` UI |
| `photon-editor` | Editor workspace UI, tool rail, tool options card, canvas scrollbars & zoom controls, layers/adjustments docks, and splitters | `iced` UI |
| `photon-cli` | `photonctl` binary: control commands, `debug` suite, and `mcp` JSON-RPC stdio server | Yes |
| `photon-desktop` | `photon` binary: `iced 0.14` (`daemon` + multi-window) shell, menus, dialogs (`ManagePlugins`, `PluginParams`, `AdjustDenoise`), semantic UI inspector, and `--headless` mode | GUI binary |

## UI, Panel & Dialog Architecture
- **Layering**: `photon-desktop` composes `photon-ui` (tokens, controls, menus, toolbars, `CurveGraphProgram`), `photon-dialogs` (OS pickers, `DialogManager<DesktopDialogId>`, `OverlayMode::{DimmedFrosted, Cutout, Floating}`), `photon-panels` (`SidebarOrderState`, `DraggablePanelHeaderSpec`, `sidebar_panel`, `sidebar_dock`), `photon-canvas`, and the 3 workspace UI crates (`photon-browser`, `photon-viewer`, `photon-editor`), while keeping all domain mutations routed through `CoreHandle::dispatch`.
- **Unified Sidebars**: Browser (`[Quick Access, Explorer]` + optional `Detail`), Viewer (`[View Adjustments, Info]`), and Editor (`[Tool Options | History, Layers]`) share identical drag-reorderable panel chrome, collapse state, and `LayoutState` persistence (`pending_layout_commands` guards against stale `watch` overwrites).
- **Live-Preview Cutouts**: Editor adjustment dialogs (`AdjustColor`, `AdjustHsv`, `AdjustColorBalance`, `AdjustCurves`, `AdjustDenoise`) use `OverlayMode::Cutout` over `resolved_editor_canvas_bounds` so perimeter UI stays locked while the `wgpu` canvas remains undimmed and pan/zoom-interactive.
- **Sandboxed Plugin Bridge & Legacy AI Command Compatibility**:
  - External plugins run out-of-process via `photon-plugins` (`<data_dir>/plugins` or `PHOTON_PLUGINS_DIR`) with `PluginRegistry` in `Core` and `Plugins` menus + `ManagePlugins` / `PluginParams` dialogs in the GUI (see [PLUGINS.md](PLUGINS.md)).
  - Legacy built-in translation commands (`ViewerTranslate`, `ViewerSaveTranslation`, `ViewerDeleteTranslation`, `SetTranslateSettings`) remain serde-compatible in `photon-protocol` and `Config` for deterministic `commands.jsonl` replay, while `Core::dispatch` rejects them with `feature_disabled` (`ai_feature_disabled`); OCR and translation workflows are provided through the standalone plugins (`photon.translate`, `photon.ocr.tesseract`, `photon.ocr.ai`).

## Modes & Multi-Window Strategy
Instead of 3 disconnected binaries (which break GPU texture sharing and cross-workspace transitions), `photon` runs **one CoreRuntime** in-process and exposes:
- **3 Workspaces** (`Browser`, `Viewer`, `Editor`) switchable instantly in-window with zero re-decode.
- **Multi-window (`iced::daemon`)**: extra windows can be opened (`--new-window` or UI action) sharing the same `CoreRuntime` and `wgpu` caches.
- **Headless mode (`photon --headless`)**: runs `CoreRuntime` + `photon-ipc` (including `PluginRun`, batch ops, and `.ora`/codec pipelines) without opening any window or GPU surface for automation, CI, and CLI control.
- **Single-instance & Independent Viewer coordination**:
  - Launching Browser/Editor (`photon`, `photon browse`, `photon edit`, `photon <dir>`) when a main instance is listening on `control.sock` forwards the command + `FocusWindow` over IPC and exits `0`.
  - Launching Viewer (`photon view <file>`, `photon <file>`, Browser open, or Viewer "Open in New Window") checks `$XDG_RUNTIME_DIR/photon[-dev]/viewers/` (`v-<pid>.json`, `v-<pid>.sock`, `CanonicalPathLock`). If a live Viewer already holds that canonical path, it sends `FocusWindow` to raise that window; otherwise it spawns an independent `StandaloneViewer` (`Core::new_viewer_ephemeral`) so Viewer windows are isolated from each other and the main app.
- **Separated Settings vs UI State**: Settings persist in `config.json` (`theme`, `language`, `remember_last_folder`, `save_ui_state_on_exit`, `hardware_acceleration`, `favorites`, `plugins`, `ai`, `translate`, and Settings-level defaults), while runtime UI state persists in `ui_state.json` (`last_folder`, window bounds/maximized, panel visibility/order/splits, `viewer_viewport_background`, sort/view modes), flushed on exit (when `save_ui_state_on_exit` or `Ctrl` force-save), `Ctrl+,`, `ResetConfig`, or `photonctl reset`.

## GPU & Large-Image Pipeline (`iced 0.14` + `wgpu 27`)
- **Renderer Selection**: At startup (`crates/photon-desktop/src/main.rs`), `core.config.hardware_acceleration == false` or `--safe-mode` selects `ICED_BACKEND=tiny-skia` (software renderer); otherwise `wgpu` is used. Settings displays the active renderer and a restart-required notice when toggled.
- **Two-Stage WGSL Canvas Pipeline (`crates/photon-canvas/src/lib.rs`)**:
  - **Stage 1 (`sample_processed_image`)**: evaluates `CanvasAntiAliasingMode` (`Crisp` Nearest / `Smooth` Bilinear / `HighQuality` 16-tap Bicubic Catmull-Rom), `ViewportDenoiseMode` (5×5 edge-preserving bilateral filter), Color & Light adjustments, `256×1` RGBA8 Curve 1D-LUT (`@binding(6)`), and live 60fps inverse 2D affine `TransformPreview` (`transform_layer_tex` at `@binding(7)`).
  - **Stage 2 (`apply_crisp_overlays`)**: composites `crop_rect`, Quick Mask, animated marching ants from `sel_mask_tex` via a dedicated **`sel_sampler` (`FilterMode::Nearest`, `ClampToEdge` at `@binding(5)`)**, selection preview rect, and SDF-anti-aliased cursor ring — preventing Stage 1 denoise/AA from blurring or creating halos around selection borders.
- **Oversized (`>4096px`) Image Alignment**:
  - GPU texture upload limit is `device.limits().max_texture_dimension_2d.min(16384)` (`MAX_GPU_TEXTURE_DIM`), while `Uniforms.image_size` / `pair_size` always carry the true document dimensions (`frame.width × frame.height`) so cursor coordinates, `SelectionMask`, and vector overlays stay 1:1 pixel-aligned at any resolution, and `photon-image` preserves full-resolution CPU truth for non-destructive edits and export.
