# `photon-viewer-canvas` (`crates/apps/photon/photon-viewer-canvas`)

Dedicated lightweight 2D canvas for `photon-viewer` and `photon-app-viewer`.

## Invariants
- Keep dependencies minimal (`iced`, `photon-protocol` only).
- Built on `iced::widget::Canvas` (`draw_image`), supporting both `tiny-skia` (CPU, default) and `wgpu` with zero WGSL shaders or Editor pipelines.
