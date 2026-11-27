# Photon App Layer (`crates/apps/photon/*`)

This directory contains Photon's domain crates (`photon-protocol`, `photon-storage`, `photon-i18n`, `photon-image`, `photon-plugins`, `photon-ipc`, `photon-canvas`, `photon-viewer-canvas`, `photon-browser`, `photon-viewer`, `photon-editor`, `photon-app-common`, `photon-desktop`, `photon-cli`, `photon-app-manager`, `photon-app-viewer`, `photon-app-editor`).

## Invariants
1. **Independent app binaries**: `photon-app-manager`, `photon-app-viewer`, and `photon-app-editor` each own their own `iced::daemon` GUI and `fw_runtime::AppCore` (`ManagerCore`, `ViewerCore`, `EditorCore`). Never cross-depend between the three app crates or re-introduce a monolithic `photon-core` / `photon-desktop` GUI.
2. **Viewer fast path**: `photon-app-viewer` and `photon-viewer` use `photon-viewer-canvas` (single minimal WGSL textured-quad pipeline), never `photon-canvas` or `photon-plugins`.
3. **Shared launcher helpers**: Cross-app CLI flags, IPC launch routing, native file dialogs, and window activation live in `photon-app-common` (zero GUI/canvas dependencies).
4. **Headless dispatcher**: `photon-desktop` (`photon` binary) is strictly a thin CLI launcher/dispatcher (`--headless` delegates to `photon-manager --headless`).
