# Photon Editor Walkthrough: Selection Suite, Navigation & Krita Drawing Tools

Implemented the full Photoshop + Krita pixel selection suite, Selection UX & algorithm fixes, standardized Pan/Zoom/`Space + Drag`/`Alt + Click` navigation, and the Krita-style raster-first Drawing & Bézier Curve toolset across `photon-protocol`, `photon-image`, `photon-core`, `photon-ui`, `photon-canvas`, `photon-i18n`, `photon-editor`, `photon-viewer`, and `photon-desktop`.

## Summary of Changes

### 1. Selection Suite & UX/Algorithm Fixes
- **Protocol, Image Engine & Core**:
  - `SelectionBooleanOp`, `SelectionShape`, `SelectionModifyOp`, `EditorSelectionSummary`, and 12 `Command::EditorSelect*` / `EditorSelection*` / `EditorCropToSelection` commands in [editor_types.rs](../crates/photon-protocol/src/editor_types.rs), [state.rs](../crates/photon-protocol/src/state.rs), and [command.rs](../crates/photon-protocol/src/command.rs).
  - Copy-on-Write `SelectionMask` in [selection.rs](../crates/photon-image/src/selection.rs) with rasterizers, boolean ops, morphological/color modifiers, Quick Mask painting, and selection clipping in [document.rs](../crates/photon-image/src/document.rs).
- **Menu Alignment & Numeric Input Dialog**:
  - Fixed `build_active_menu` X-offsets (`EditorSelect: 152.0`, `EditorFilter: 208.0`, `EditorView: 258.0`, `EditorWindow: 308.0`, `EditorHelp: 376.0`) and added `DesktopDialogId::SelectionModify` (`PromptDialogSpec` with range validation for `Feather`, `Expand`, `Contract`, `Border`, `Smooth`, `Grow`, `Similar`) in [app.rs](../crates/photon-desktop/src/app.rs).
- **GPU Vector Overlay, Krita Polygon & Magnetic Lasso**:
  - Added a `wgpu` vector overlay sub-pipeline (`overlay_line_pipeline`, `overlay_tri_pipeline`) in [photon-canvas/src/lib.rs](../crates/photon-canvas/src/lib.rs) for 60fps polylines, rubber-band lines, vertex anchors, and close-snap rings (`<= 10.0 / zoom`).
  - Added Krita-style `PolygonLasso` controls (click, snap-ring / double-click / `Enter` commit, `Backspace` undo vertex, `Esc` cancel) and Sobel + 8-connected Dijkstra `magnetic_snap` for `MagneticLasso`.

### 2. Standardized Pan, Scroll-Zoom, Global `Space + Drag` & `Alt + Click`
- Added `restrict_pan_to_hand: bool` (`true` in Editor, `false` in Viewer) and `space_pan_active` in [photon-canvas/src/lib.rs](../crates/photon-canvas/src/lib.rs), [photon-viewer/src/lib.rs](../crates/photon-viewer/src/lib.rs), and [photon-editor/src/lib.rs](../crates/photon-editor/src/lib.rs).
- Added `clamp_editor_pan` on `PanBy` and `ZoomBy` in [app.rs](../crates/photon-desktop/src/app.rs).
- `Space + Drag` pans without clearing in-progress Polygon/Bézier nodes; `Alt + Click` emits `SampleColor` across drawing/painting/pointer tools while preserving `Alt` = `Subtract` for Selection tools.

### 3. Krita-style Drawing & Bézier Curve Toolset
- Added `BrushSmoothingMode` (`None`, `Basic`, `Weighted`, `Stabilizer`), `VectorPathNode`, `DrawShapeKind`, `ShapeFillMode`, `GradientKind`, `GradientColorMode`, and `Command::{EditorDrawShape, EditorFloodFill, EditorDrawGradient}`.
- Implemented [drawing.rs](../crates/photon-image/src/drawing.rs) (De Casteljau cubic Bézier tessellation, `paint_brush_dab` path stroking, 4x4 supersampled scanline fill, brush smoothing, flood fill, and 4 gradient modes) with `SelectionMask` clipping, Layer Mask / Quick Mask support, and Undo/Redo in [document.rs](../crates/photon-image/src/document.rs) and [core.rs](../crates/photon-core/src/core.rs).
- Added 8 icons in [icons.rs](../crates/photon-ui/src/icons.rs), full `en-US` / `vi-VN` localization, interactive `Shape` / `Stabilizer` / `BezierPen` GPU overlay editing, 3 tool rail slots (`U`, `P`, `F`/`G`), tool options, and semantic nodes in [inspector.rs](../crates/photon-desktop/src/inspector.rs).

## Verification
- `cargo fmt --all -- --check && just fmt-check && just clippy && just test`: passed (exit 0; 98 tests passed, 0 failed across all crates).
- Live GPU X11 verification (`wgpu`/Vulkan Intel Iris Xe): verified aligned `Select` menu, `Feather...`..`Similar...` enablement, Tool Rail groups, and Tool Options for `Brush` smoothing, `Shape`, `BezierPen`, `PaintBucket`, and `Gradient`.
