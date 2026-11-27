# `photon-app-viewer` (`crates/apps/photon/photon-app-viewer`)

Standalone, latency-optimized image viewer (`photon-viewer` binary).

## Invariants
- Owns its own `ViewerCore` (`fw_runtime::AppCore`) and `ViewerApp` (`iced::daemon`).
- Must NEVER depend on `photon-desktop`, `photon-browser`, `photon-editor`, `photon-canvas`, or `photon-plugins`.
- Keep startup minimal: lazy-load filmstrip thumbnails only when open, pre-decode adjacent pages on `BG_RUNTIME`, and default `WGPU_BACKEND=vulkan` on Linux when unset.
