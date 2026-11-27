# `photon-viewer-canvas` (`crates/apps/photon/photon-viewer-canvas`)

Dedicated lightweight `wgpu` canvas for `photon-viewer` and `photon-app-viewer`.

## Invariants
- Keep dependencies minimal (`iced`, `photon-protocol` only).
- Never add Editor-only pipelines (layers, brushes, marching ants, vector overlays, crop handles, curve LUTs) so first-launch shader compilation stays fast.
