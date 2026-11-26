# Photon Editor & Workspace Walkthrough

This walkthrough summarizes the major capabilities implemented across `photon-protocol`, `photon-storage`, `photon-image`, `photon-core`, `photon-plugins`, `photon-ipc`, `photon-ui`, `photon-panels`, `photon-canvas`, `photon-i18n`, `photon-browser`, `photon-viewer`, `photon-editor`, `photon-cli`, and `photon-desktop`.

## Summary of Capabilities

### 1. Selection Suite, Selection-Clipped Brush/Eraser & Free Transform
- **Selection & Masks**: `SelectionMask` ([selection.rs](../crates/photon-image/src/selection.rs)) supports Marquee (`M`), Freehand/Polygon/Magnetic/Bézier Lasso, Magic Wand (`W`), Quick Select, Color Range, Quick Mask (`Q`), boolean ops, `SelectionModify` dialog, and `Nearest`-sampled GPU marching ants (`sel_sampler`).
- **Live Stroke & Clipping**: `Brush`/`Eraser` live strokes clip to `SelectionMask` (with feathering), offset layers, and layer transparency masks (`document.rs`).
- **Selection-Aware Free Transform (`Ctrl+T`)**: Unclamped canvas handle hit-testing (`screen_to_image_px_unclamped`), premultiplied GPU inverse-affine preview, live transformed marching ants (`sample_sel_mask_at`), and mask/baseline preservation on expansion.

### 2. Multi-Stop Gradient Engine & Interactive Preview (`G`)
- Ported KogaView's multi-stop gradient engine (`render_gradient` / `render_gradient_on_mask` in [drawing.rs](../crates/photon-image/src/drawing.rs)): 6 shapes (`Linear`, `Bilinear`, `Radial`, `Square`, `Conical`, `Spiral`), 3 repeat modes (`None`, `Forwards`, `Alternating`), 7 blend modes, 4×4 Bayer dithering, selection clipping, and grayscale layer-mask rendering.
- Interactive `wgpu` preview with draggable start/end handles, `Shift` 15° snapping, `Enter`/`Esc` draft commit/cancel, 8 presets (`FG -> BG`, `FG -> Clear`, `Rainbow`, `Sunset`, `Ocean`, `Fire`, `Chrome`, `Vignette`), and interactive gradient stop editor.

### 3. Multi-Tab Editor, New Document (`Ctrl+N`), Unsupported Format Screen & Virtual Save As
- **Image Size & Canvas Size**: Moved out of the left tool rail into the **Image** menu and canvas context menu (`Ctrl+R` Image Size, `Ctrl+Shift+R` Canvas Size), opening as modal dialogs with `ColorPickerTarget::CanvasSizeFill`.
- **Multi-Tab Editor**: `document_tab_bar` below the Editor toolbar with `+` / `Ctrl+N` new document creation, virtual `Save As` for unsaved/new documents, Unsupported Format screen (`Command::OpenInEditorOrUnsupported` with `Back to Viewer` / `Back to Browser`), dirty indicators, per-tab view cache (`zoom`, `pan`, `crop_uv`, `tool`, `pending_adj`), dirty-close confirmation (`Ctrl+W`), and `Ctrl+Tab`/`Alt+1..9` switching.

### 4. Translate Plugin & Pipeline Parity
- **Model & Tessdata Resolution**: `resolve_models_dir_for` ([bridge.rs](../crates/photon-plugins/src/bridge.rs)) resolves plugin model directories consistently; OCR and Translate plugins pass `--tessdata-dir` to Tesseract.
- **Provider & Format Parity**: `TranslateProvider::GoogleWeb` is the default provider; `rasterize_for_translate` ([ai.rs](../crates/photon-core/src/ai.rs)) decodes non-standard image formats (`.jp2`/`.heic`/`.hif`/`.avif`/`.jxl`/`.kra`/Camera RAW) to temporary PNGs for OCR/translation and cleans up on drop.
- **KogaView Plugin-First Routing & Overlay**: KogaView plugin-first routing, `ViewerMessage::SwapTranslateLangs` with Tesseract↔ISO language mapping, block-merge and per-path caching, rotated/Dual/Manhwa overlay rendering, `⋮⋮` drag handles, bottom-centered `result_card`, and full-box text masking with high-contrast `pal.text_primary`.

### 5. Popup/Dialog & Menu/Icon Polish
- **Dialog Sizing & Resize**: `resolve_card_and_body_limits` ([frame.rs](../crates/photon-dialogs/src/frame.rs)) clamps dialog card and scrollable body dimensions cleanly across viewport sizes alongside 8-way resize handles.
- **Menus & Icons**: `MenuEntry::IconRow` and `opaque(scrollable(...))` in `photon-ui` ([menu.rs](../crates/photon-ui/src/menu.rs)), `Icon::BookOpen` (`ReaderOptions`) and `Icon::PanelsTopLeft` (`Filmstrip`) in [icons.rs](../crates/photon-ui/src/icons.rs), Quick Access header pencil Edit toggle, and responsive `ReaderOptions` popup.

### 6. Extended Codecs & Hybrid Archive/PDF (`ArchiveSession`)
- **Native & Extended Codecs**: Added `.jp2`/`.j2k`/`.jpx`/`.jpf` (`libopenjp2`) and `.heic`/`.heif`/`.hif` (`libheif`) via [native_codecs.rs](../crates/photon-image/src/native_codecs.rs), `avif-native`, 29 `rawler` Camera RAW formats, `.kra` (Krita), and `.icns` (`image/x-icns` in [codec.rs](../crates/photon-image/src/codec.rs) via `extract_icns_best_payload`).
- **Hybrid Archive/PDF**: `ArchiveSession` ([archive.rs](../crates/photon-image/src/archive.rs)) indexes `.cbz`/`.zip`/`.pdf` pages lazily and materializes requested pages on demand, paired with a windowed Filmstrip in Viewer and Open / Edit / `xdg-open` actions in Browser.

### 7. 3-Process Architecture & Linux Taskbar Separation
- **3-Process Routing**: `compute_launch_route` ([main.rs](../crates/photon-desktop/src/main.rs)) routes launches across:
  - **Browser singleton** (`photon.sock`)
  - **Editor singleton** (`editor.sock` via `AppPaths::editor_socket_path`)
  - **Viewer multi-instance** (`v-<pid>.sock` + `CanonicalPathLock` focus+restore deduplication)
- **Viewer Toolbar & Linux Taskbar Grouping**: Viewer toolbar groups `[ Browser | Editor ]` and `Filmstrip → Fullscreen → Info → Adjustment`. `window_app_id_for` ([app.rs](../crates/photon-desktop/src/app.rs)) sets distinct `application_id` / `StartupWMClass` values (`io.github.tienv2i.Photon`, `io.github.tienv2i.Photon.Editor`, `io.github.tienv2i.Photon.Viewer`) matching the 3 `.desktop` templates and MIME defaults in [scripts/install.py](../scripts/install.py). Each role also ships its own icon name (`io.github.tienv2i.Photon` grid, `.Viewer` framed photo + lens, `.Editor` sliders) so taskbar and app-menu entries are visually distinct.
