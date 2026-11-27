# Photon App Layer (`crates/apps/photon/*`)

This directory contains Photon's domain crates (`photon-protocol`, `photon-storage`, `photon-i18n`, `photon-image`, `photon-plugins`, `photon-core`, `photon-ipc`, `photon-canvas`, `photon-browser`, `photon-viewer`, `photon-editor`, `photon-desktop`, `photon-cli`, `photon-app-*`).

## App-specific invariants
1. **3-Process Window Strategy**:
   - Browser/Manager singleton (`control.sock`), Editor singleton (`editor.sock`), Viewer multi-instance (`viewers/v-<pid>.sock` + `CanonicalPathLock`). All share `PHOTON_BRANDING` (`photon` / `photon-dev`).
2. **Fast Viewer Folder Scan**:
   - `StandaloneViewer` (`viewer_ephemeral = true`) and `sync_viewer_from_active_editor_doc` must use `Core::scan_viewer_folder` (extension-first + `DirEntry::file_type()`, no `fs::metadata` / Browser `Entry` allocation).
3. **i18n Parity**:
   - Every `en-US` key and placeholder in `photon-i18n/locales/` must match `vi-VN` (`cargo test -p photon-i18n`).
4. **Non-destructive Editing & GPU Pipeline**:
   - Overwriting an image stages to a temp file and keeps `file~` backups; `photon-canvas` keeps Stage 1 (AA/denoise/color/LUT) separate from Stage 2 (crisp selection/cursor overlays).

## Verification
- `just fmt-check && just clippy && just test`
