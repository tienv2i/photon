# `photon-app-manager` (`crates/apps/photon/photon-app-manager`)

Standalone file manager & browser (`photon-manager` binary), also hosting `--headless` IPC mode.

## Invariants
- Owns its own `ManagerCore` (`fw_runtime::AppCore`) and `ManagerApp` (`iced::daemon`).
- Must NEVER depend on `photon-desktop`, `photon-viewer`, `photon-viewer-canvas`, `photon-editor`, or `photon-canvas`.
- `File -> Open` / Open Folder always picks a folder (`pick_folder`); opening an image launches `photon-viewer` or `photon-editor` as a separate process.
