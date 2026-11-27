# `photon-app-viewer` (`crates/apps/photon/photon-app-viewer`)

Standalone, latency-optimized image viewer (`photon-viewer` binary).

## Invariants
- Owns its own `ViewerCore` (`fw_runtime::AppCore`) and `ViewerApp` (`iced::daemon`).
- Must NEVER depend on `photon-desktop`, `photon-browser`, `photon-editor`, `photon-canvas`, or `photon-plugins`.
- Defaults to `ICED_BACKEND=tiny-skia` (zero Vulkan/WGSL warm-up), pre-decodes the initial image on `BG_RUNTIME` before `iced::daemon` opens the window, and uses `fast_exit()` after cleaning up `v-<pid>.sock`/`.json`.
