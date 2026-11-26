# Photon Project State

Verified snapshot of the `/home/huynhat/Projects/photon` workspace.

## Workspace Inventory

- **Workspace members**: 16 crates (`edition = "2024"`, `rust-version = "1.88"`, `license = "MIT"`):
  - Core, plugins & automation: `photon-protocol`, `photon-storage`, `photon-i18n`, `photon-image`, `photon-core`, `photon-plugins`, `photon-ipc`, `photon-cli`
  - UI, GPU canvas & workspaces: `photon-dialogs`, `photon-ui`, `photon-panels`, `photon-canvas`, `photon-browser`, `photon-viewer`, `photon-editor`, `photon-desktop`
- **Standalone plugin crates (`plugins/`)**: 9 crates (`license = "MIT"`):
  - `photon-plugin-ai-inpaint`, `photon-plugin-ai-ocr`, `photon-plugin-caption`, `photon-plugin-dewatermark`, `photon-plugin-ocr`, `photon-plugin-rembg`, `photon-plugin-translate`, `photon-plugin-upscale`, `photon-plugin-utils`
- **Binaries**: `photon` (`crates/photon-desktop`) and `photonctl` (`crates/photon-cli`)

## Verified Checks (Current Tree)

All workspace checks below were executed after the final Rust edits:

| Check | Command | Result |
|---|---|---|
| Formatting | `just fmt-check` (`cargo fmt --all -- --check`) | Passed |
| Workspace tests | `just test` (`cargo test --workspace`) | **167 passed, 0 failed** (`photon-browser`: 5, `photon-canvas`: 13, `photon-cli`: 1, `photon-core`: 21 unit + 8 `automation_e2e`, `photon-desktop`: 6, `photon-dialogs`: 11, `photon-editor`: 5, `photon-i18n`: 3, `photon-image`: 29, `photon-ipc`: 4, `photon-panels`: 13, `photon-plugins`: 12, `photon-protocol`: 10, `photon-storage`: 8, `photon-ui`: 17, `photon-viewer`: 1) |

## Implemented Feature Milestones

1. **Workspace Crate Extraction**: Extracted `photon-canvas`, `photon-browser`, `photon-viewer`, `photon-editor`, and `photon-plugins` as clean workspace crates.
2. **Photoshop + Krita Selection Suite & Vector Overlay**:
   - `SelectionMask` CoW mask (`photon-image/src/selection.rs`), boolean ops, morphological/color modifiers, Quick Mask (`Q`), animated GPU marching ants shader, `Select` menu + `SelectionModify` dialog, GPU Vector Overlay sub-pipeline, Krita-style `PolygonLasso`, and Sobel + Dijkstra `magnetic_snap` for `MagneticLasso`.
3. **Krita-style Drawing & Bézier Curve Toolset**:
   - `BrushSmoothingMode` (`None`, `Basic`, `Weighted`, `Stabilizer`), `Shape` tools (`U`), interactive `Bézier Pen` (`P`), `Paint Bucket` (`F`), and `Gradient` (`G`).
4. **Interactive Curves + Live GPU 1D-LUT**:
   - `CurveGraphProgram` (`crates/photon-ui/src/curve_editor.rs`) embedded in `DesktopDialogId::AdjustCurves` (`460×520`) with luminance histogram backdrop, channel reset, 5 presets, and real-time `256×1` RGBA8 GPU LUT (`@binding(6)`).
5. **Complete Fill, Gradient & Spot/Sampled Healing Brush**:
   - Propagated live FG/BG colors, opacity, `sample_all_layers`, `reverse`, and 4×4 Bayer `dither`; exposed `PaintBucket` (`F`), `Gradient` (`G`), and `HealingBrush` (`J`) as 3 separate rail buttons; implemented `healing_stroke` / `healing_stroke_with_reference` (`HealingMode::{Spot, Sampled}`) with `SelectionMask` clipping and `Alt+Click` source picking.
6. **Sandboxed Plugin Bridge & 9 Ported Plugins**:
   - `crates/photon-plugins` supporting native `plugin.json` (`PhotonNdjsonV1`) and KogaView `manifest.json` (`JsonlStdioV1`), `Plugins` menus in Browser & Editor, `ManagePlugins` (`640×480`) + `PluginParams` dialogs, `--headless`/replay support, and 9 standalone plugins under `plugins/` + `just build-plugins` / `just install-plugins`.
7. **Browser Launch State, Hardware Acceleration, Viewport AA/Denoise & Selection Isolation**:
   - `AppLaunchWindowMode::{Maximized, Remembered}` (first-run maximized Browser), `Config.hardware_acceleration` + `--safe-mode` (`wgpu` vs `tiny-skia`), `CanvasAntiAliasingMode` + `ViewportDenoiseMode`, selection-aware `denoise_rgba` (`Filter -> Denoise…` / `LayerEffect::Denoise`), and dedicated `sel_sampler` (`FilterMode::Nearest` `@binding(5)`) isolating Stage 2 selection overlays from Stage 1 image denoise/AA.
8. **Browser/Editor Copy & Paste, OpenRaster (`.ora`) & Extended Codecs**:
   - Browser `Ctrl+C`/`Ctrl+V` (`Command::PasteImageBytes`, `stem (copy).ext`) and Editor selection/layer-aware `Cut`/`Copy`/`Copy Merged`/`Paste`/`Paste Into Selection` (`EditorPixelClipboard`, `arboard` + Wayland/X11 CLI fallback, `Query::EditorClipboardPng`).
   - Pure-Rust `.ora` multi-layer read/write (`crates/photon-image/src/ora.rs`), `.psd` multi-layer import, `.svg`/`.svgz`, `.jxl`, Camera RAW (`rawler`), `.avif`, `.exr`, `.hdr`, `.ico`, `.cur`, `.pam`, `.dds`, `.ff`, `.apng`, optional `.heic`/`.heif` CLI fallback, and expanded `SaveFormat`.
9. **Free Canvas Navigation (`>4096px` GPU Alignment) & Live 60fps Free Transform (`Ctrl+T`)**:
   - Tool-safe free pan (Middle-mouse & `Space+Drag` across all tools, `Unfocused` flag reset), dynamic zoom range (`min_zoom = 0.02`, `max_zoom = (32.0 / fit_scale).max(32.0)`), Editor canvas scrollbars + `-`/`%`/`+`/`Fit`/`1:1` controls, `MAX_GPU_TEXTURE_DIM = max_texture_dimension_2d.min(16384)` with exact `doc_size` coordinate alignment, and live GPU `TransformPreview` (`@binding(7)`) + OBB handles committing via `Command::EditorTransformLayer`, plus selection-aware Transform fixes (`SimpleFrameKey`, premultiplied sampling, live transformed marching ants, unclamped handles).
10. **Multi-Stop Gradient, Multi-Tab Editor, Independent Viewers & Separated Persistence**:
    - KogaView multi-stop Gradient engine (6 shapes, 3 repeat modes, 7 blend modes, 8 presets, interactive stop editor, live `wgpu` preview), selection-clipped live Brush/Eraser strokes.
    - Image Size & Canvas Size moved from tool rail into Image menu & popups; Multi-tab Editor (`document_tab_bar`, per-tab view cache, dirty-close prompt); Independent multi-image Viewer processes (`photon-ipc::viewer_registry` canonical-path deduplication & focus); Quick Access folder icons + hidden-by-default Edit toggle; auto-closing `Workspace & View` menu with Viewer Canvas Backdrop; `config.json` vs `ui_state.json` separation with `save_ui_state_on_exit`, `Ctrl+Q`/`Ctrl+Close` force-save, `photonctl reset <layout|settings|all>`, and Safe Mode disk reset.

## Verification Scope Notes

- Automated `just test` validates domain logic, protocol schemas, IPC, replay, image processing (`selection`, `drawing`, `healing`, `ora`, codecs, `denoise_rgba`, affine transforms), plugin bridge (`PhotonNdjsonV1` & `JsonlStdioV1`), i18n parity (756 keys), and UI/canvas/transform math.
- Live GPU/window rendering (`wgpu` / `iced::daemon`) is verified interactively using `photon` + `photonctl debug ui-tree` + `photonctl debug screenshot` per [WORKFLOW.md](WORKFLOW.md).
