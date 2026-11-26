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
| Workspace tests | `just test` (`cargo test --workspace`) | **225 passed, 0 failed** (`photon-browser`: 7, `photon-canvas`: 13, `photon-cli`: 1, `photon-core`: 33 unit + 8 `automation_e2e`, `photon-desktop`: 20, `photon-dialogs`: 12, `photon-editor`: 5, `photon-i18n`: 3, `photon-image`: 35, `photon-ipc`: 8, `photon-panels`: 13, `photon-plugins`: 1 unit + 12 `plugin_bridge_tests`, `photon-protocol`: 13, `photon-storage`: 13, `photon-ui`: 21, `photon-viewer`: 7) |

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
   - `AppLaunchWindowMode::{Maximized, Remembered}` (Maximized is the default for Browser/Editor; Viewer continues to use `SystemOpenWindowMode`/FitImage), `Config.hardware_acceleration` + `--safe-mode` (`wgpu` vs `tiny-skia`), `CanvasAntiAliasingMode` + `ViewportDenoiseMode`, selection-aware `denoise_rgba` (`Filter -> Denoise…` / `LayerEffect::Denoise`), and dedicated `sel_sampler` (`FilterMode::Nearest` `@binding(5)`) isolating Stage 2 selection overlays from Stage 1 image denoise/AA.
8. **Browser/Editor Copy & Paste, OpenRaster (`.ora`) & Extended Codecs**:
   - Browser `Ctrl+C`/`Ctrl+V` (`Command::PasteImageBytes`, `stem (copy).ext`) and Editor selection/layer-aware `Cut`/`Copy`/`Copy Merged`/`Paste`/`Paste Into Selection` (`EditorPixelClipboard`, `arboard` + Wayland/X11 CLI fallback, `Query::EditorClipboardPng`).
   - Pure-Rust `.ora` multi-layer read/write (`crates/photon-image/src/ora.rs`), `.psd` multi-layer import, `.svg`/`.svgz`, `.jxl`, Camera RAW (`rawler`), `.avif`, `.exr`, `.hdr`, `.ico`, `.cur`, `.pam`, `.dds`, `.ff`, `.apng`, optional `.heic`/`.heif` CLI fallback, and expanded `SaveFormat`.
9. **Free Canvas Navigation (`>4096px` GPU Alignment) & Live 60fps Free Transform (`Ctrl+T`)**:
   - Tool-safe free pan (Middle-mouse & `Space+Drag` across all tools, `Unfocused` flag reset), dynamic zoom range (`min_zoom = 0.02`, `max_zoom = (32.0 / fit_scale).max(32.0)`), Editor canvas scrollbars + `-`/`%`/`+`/`Fit`/`1:1` controls, `MAX_GPU_TEXTURE_DIM = max_texture_dimension_2d.min(16384)` with exact `doc_size` coordinate alignment, and live GPU `TransformPreview` (`@binding(7)`) + OBB handles committing via `Command::EditorTransformLayer`, plus selection-aware Transform fixes (`SimpleFrameKey`, premultiplied sampling, live transformed marching ants, unclamped handles).
10. **Multi-Stop Gradient, Multi-Tab Editor, Independent Viewers & Separated Persistence**:
    - KogaView multi-stop Gradient engine (6 shapes, 3 repeat modes, 7 blend modes, 8 presets, interactive stop editor, live `wgpu` preview), selection-clipped live Brush/Eraser strokes.
    - Image Size & Canvas Size moved from tool rail into Image menu & popups; Multi-tab Editor (`document_tab_bar`, per-tab view cache, dirty-close prompt); Independent multi-image Viewer processes (`photon-ipc::viewer_registry` canonical-path deduplication & focus); Quick Access folder icons + hidden-by-default Edit toggle; auto-closing `Workspace & View` menu with Viewer Canvas Backdrop; `config.json` vs `ui_state.json` separation with `save_ui_state_on_exit`, `Ctrl+Q`/`Ctrl+Close` force-save, `photonctl reset <layout|settings|all>`, and Safe Mode disk reset.
11. **Translate Pipeline Parity, Extended Codecs, Hybrid Archive/PDF & 3-Process Taskbar Separation**:
    - Translate plugin & pipeline parity: `resolve_models_dir_for`, `--tessdata-dir`, `GoogleWeb` default, `rasterize_for_translate` for JP2/HEIC/HIF/AVIF/JXL/KRA/RAW, KogaView plugin-first routing, `SwapTranslateLangs` Tesseract↔ISO mapping, block-merge/per-path cache, rotated/Dual/Manhwa overlay, `⋮⋮` drag handles, bottom-centered `result_card`, full-box text masking + `pal.text_primary` high contrast.
    - Popup/dialog & menu/icon polish: `resolve_card_and_body_limits` + resize handles (`photon-dialogs`), `opaque(scrollable(...))` + `MenuEntry::IconRow` (`photon-ui`), `Icon::BookOpen` & `Icon::PanelsTopLeft`, Quick Access header pencil toggle, responsive `ReaderOptions`.
    - Extended codecs & hybrid archive/PDF: `.jp2`/`.j2k`/`.jpx`/`.jpf` (`libopenjp2`), `.heic`/`.heif`/`.hif` (`libheif`), `avif-native`, 29 `rawler` RAWs, `.kra`, `.icns` (`image/x-icns`), `ArchiveSession` (`.cbz`/`.zip`/`.pdf`) + virtualized Filmstrip (visible-range thumbnails, follows active page, resize-aware), Browser Open/Edit/`xdg-open`, Editor `New` (`Ctrl+N`), Unsupported Format screen, virtual `Save As`.
    - 3-process architecture & Linux taskbar separation: Browser singleton (`photon.sock`), Editor singleton (`editor.sock`), Viewer multi-instance (`v-<pid>.sock` with focus+restore), Viewer `[ Browser | Editor ]` + `Filmstrip → Fullscreen → View Adjustments → Info` (collapsing into one `…` dropdown below 720 px), distinct `application_id`/`StartupWMClass` (`io.github.tienv2i.Photon`, `.Editor`, `.Viewer`), per-role icons (`io.github.tienv2i.Photon` grid, `.Viewer` framed photo + lens, `.Editor` sliders), and MIME defaults (`scripts/install.py`).
12. **Window Wake/Focus & Launch Maximization**:
    - Re-opening an already-open file, or switching Viewer → Browser/Editor, now forwards `Command::FocusWindow` and runs a wake sequence (`minimize(false)`, clear attention, `gain_focus`, brief `AlwaysOnTop` pulse) without changing the window's saved maximized/windowed/fullscreen state.
    - Because `iced`/`winit` cannot raise or unminimize on Wayland, `photon-desktop::window_activation` adds a KDE Plasma (KWin) D-Bus scripting fallback (`qdbus6`/`qdbus` `org.kde.KWin.Scripting`) that unminimizes and activates the window by PID. X11 and non-KDE paths keep the native iced behavior.
    - Browser/Editor launch maximized by default (`AppLaunchWindowMode::Maximized`), including `photon edit <file>` spawned from a Viewer; `Remembered` still restores the last window state, and standalone Viewer keeps `SystemOpenWindowMode` sizing.
13. **Startup Performance & Launch Safety**:
    - `boot()` no longer walks a whole folder: image probing is limited to a ±8 window around the active viewer page, Browser thumbnails are scheduled from the observed scroll viewport (about 120 entries initially, 24 in flight, 512-texture retention cap), and Viewer windows preload only their page neighbourhood. A 20k-image Browser folder now reaches first frame in ~1.3 s with ~385 MB RSS (previously ~6.1 s and ~5.1 GB), and a single-image Viewer in a 20k-image folder in ~0.2 s / ~119 MB (previously ~2.6 s / ~2.6 GB).
    - Browser/Editor launches claim a per-socket `flock` lock and pre-bind the IPC socket before `Core`/GUI init; simultaneous launches forward to the winner instead of opening duplicate windows, stale sockets are reclaimed, and launch-time IPC forwarding is bounded (500 ms connect / 2 s response) instead of hanging behind a busy core.
    - `photon %U` is now the Browser app-menu entry (no image MIME types), while `io.github.tienv2i.Photon.Viewer.desktop` (`--viewer-window`, `.Viewer` WMClass) is the default image handler installed by `scripts/install.py --set-default`. Legacy configs (`schema_version` absent) with the old `remembered` + `windowMaximized=true` default migrate once to `Maximized`.

## Verification Scope Notes

- Automated `just test` validates domain logic, protocol schemas, IPC, replay, image processing (`selection`, `drawing`, `healing`, `ora`, codecs, `denoise_rgba`, affine transforms), plugin bridge (`PhotonNdjsonV1` & `JsonlStdioV1`), i18n parity (756 keys), and UI/canvas/transform math.
- Live GPU/window rendering (`wgpu` / `iced::daemon`) is verified interactively using `photon` + `photonctl debug ui-tree` + `photonctl debug screenshot` per [WORKFLOW.md](WORKFLOW.md).
