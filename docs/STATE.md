# Photon Project State

Verified snapshot of the `/home/huynhat/Projects/photon` workspace (commit `f6f0c2e` on `main`).

## Workspace Inventory

- **Workspace members**: 15 crates (`edition = "2024"`, `rust-version = "1.88"`):
  - Core & automation: `photon-protocol`, `photon-storage`, `photon-i18n`, `photon-image`, `photon-core`, `photon-ipc`, `photon-cli`
  - UI, GPU canvas & workspaces: `photon-dialogs`, `photon-ui`, `photon-panels`, `photon-canvas`, `photon-browser`, `photon-viewer`, `photon-editor`, `photon-desktop`
- **Binaries**: `photon` (`crates/photon-desktop`) and `photonctl` (`crates/photon-cli`)

## Verified Checks (Current Tree)

All checks below were executed against the workspace after the final edit and passed:

| Check | Command | Result |
|---|---|---|
| Formatting | `cargo fmt --all -- --check && just fmt-check` | Passed |
| Lints (strict) | `just clippy` (`cargo clippy --workspace --all-targets -- -D warnings`) | Passed (0 warnings) |
| Workspace tests | `just test` (`cargo test --workspace`) | **98 passed, 0 failed** |
| Live GPU X11 | `photon` + `photonctl` (`wgpu`/Vulkan Intel Iris Xe) | Passed (Select menu, Tool Rail, Tool Options) |

## Implemented Feature Milestones

1. **Workspace Crate Extraction**: Extracted `photon-canvas`, `photon-browser`, `photon-viewer`, and `photon-editor` from `photon-desktop`.
2. **Photoshop + Krita Selection Suite**:
   - `SelectionMask` CoW mask (`photon-image/src/selection.rs`), boolean ops, morphological/color modifiers, Quick Mask (`Q`), animated GPU marching ants shader, `Select` menu + `SelectionModify` numeric input dialog, and `SelectionMask` clipping across paint/draw/adjustment ops.
   - GPU Vector Overlay sub-pipeline (`wgpu` `LineList` / `TriangleList`), Krita-style `PolygonLasso` controls, and Sobel + 8-connected Dijkstra `magnetic_snap` for `MagneticLasso`.
3. **Standardized Editor Navigation**:
   - `restrict_pan_to_hand: true` in Editor (`false` in Viewer), `clamp_editor_pan` against viewport bounds, scroll-to-zoom at cursor, global spring-loaded `Space + Drag` panning (preserving in-progress Polygon/Bézier nodes), and global `Alt + Click` color sampling for paint/draw tools.
4. **Krita-style Drawing & Bézier Curve Toolset**:
   - `BrushSmoothingMode` (`None`, `Basic`, `Weighted`, `Stabilizer`), `Shape` tools (`Line`, `Rect`, `Ellipse`, `Polygon`, `Polyline` — `U`), interactive `Bézier Pen` (`P`, `Enter` stroke/fill, `Ctrl+Enter` Path → Selection), `Paint Bucket` (`F`), and `Gradient` (`G`: `Linear`, `Radial`, `Conical`, `Reflected`).

## Verification Scope Notes

- Automated `just test` validates domain logic, protocol schemas, IPC, replay, image processing (`selection`, `drawing`), i18n parity, and UI/canvas math (`magnetic_snap`, `clamp_editor_pan`).
- Live GPU/window rendering (`wgpu` / `iced::daemon`) is verified interactively using `photon` + `photonctl debug ui-tree` + `photonctl debug screenshot`. (`SelectionModify` dialog range validation is covered by `photon-desktop` unit tests.)
