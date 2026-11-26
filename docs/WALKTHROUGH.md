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

### 3. Multi-Tab Editor, Menu-Driven Resize/Canvas Size & Independent Viewers
- **Image Size & Canvas Size**: Moved out of the left tool rail into the **Image** menu and canvas context menu (`Ctrl+R` Image Size, `Ctrl+Shift+R` Canvas Size), opening as modal dialogs with `ColorPickerTarget::CanvasSizeFill`.
- **Multi-Tab Editor**: `document_tab_bar` below the Editor toolbar with `+` new-tab picker, dirty indicators, per-tab view cache (`zoom`, `pan`, `crop_uv`, `tool`, `pending_adj`), dirty-close confirmation (`Ctrl+W`), and `Ctrl+Tab`/`Alt+1..9` switching.
- **Independent Viewer Processes**: `photon view <file>` / Browser double-click / Viewer "Open in New Window" coordinate via `$XDG_RUNTIME_DIR/photon[-dev]/viewers/` (`v-<pid>.json`, `v-<pid>.sock`, `CanonicalPathLock`) to focus an existing Viewer window if the canonical path is open, or spawn an isolated `StandaloneViewer` process.

### 4. KogaView Plugin Host Parity, 5-Tab Settings & Separated Persistence
- **Plugin Host & AI Worker**: Supports `ViewerAction`, `QuickAction`, `models.rs` auto-download/custom model paths, async AI worker progress, OCR/Translate & Caption sidecars, and `just build-plugins` / `just install-plugins` (`libonnxruntime` bundling).
- **5-Tab Settings**: `General`, `Viewer`, `Quick Access`, `Plugins` (with per-plugin `PluginModels` & `TranslateSettings` modals), and `Translate`; Safe Mode includes **Apply Safe Defaults to Disk**.
- **Workspace & View Menu + Separated UI State**: Auto-closing `Workspace & View` toolbar menu (with Viewer **Canvas Backdrop** presets & custom picker), Quick Access folder icons + hidden-by-default Edit toggle, and `config.json` (Settings) vs `ui_state.json` (UI State) separation with `save_ui_state_on_exit`, `Ctrl+Q` / `Ctrl+Close` force-save, and `photonctl reset <layout|settings|all>`.
