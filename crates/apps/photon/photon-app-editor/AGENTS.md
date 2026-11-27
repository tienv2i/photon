# `photon-app-editor` (`crates/apps/photon/photon-app-editor`)

Standalone non-destructive layered image editor (`photon-editor` binary).

## Invariants
- Owns its own `EditorCore` (`fw_runtime::AppCore`) and `EditorApp` (`iced::daemon`).
- Must NEVER depend on `photon-desktop`, `photon-browser`, `photon-viewer`, or `photon-viewer-canvas`.
- All blocking pixel/export work runs off `iced`'s internal runtime (`BG_RUNTIME`).
