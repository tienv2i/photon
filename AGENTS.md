# AI Agent Instructions — Photon (`io.github.tienv2i.Photon`)

Read `docs/ARCHITECTURE.md`, `docs/CLI.md`, and `docs/WORKFLOW.md` before implementing features.

## Workspace Layering
1. **Framework (`crates/framework/fw-*`)**: Domain-agnostic (`fw-protocol`, `fw-storage`, `fw-runtime`, `fw-ipc`, `fw-cli`, `fw-ui`, `fw-panels`, `fw-dialogs`, `fw-shell`). Must NEVER depend on `crates/apps/*` or `crates/components/*`.
2. **Components (`crates/components/comp-*`)**: Reusable domain components (`comp-file-browser` with zero-stat NTFS-optimized directory scanning).
3. **Apps (`crates/apps/*`)**:
   - `crates/apps/photon/photon-app-viewer` (`ViewerCore` + `photon-viewer` + `photon-viewer-canvas`)
   - `crates/apps/photon/photon-app-manager` (`ManagerCore` + `photon-browser`, also hosts `--headless`)
   - `crates/apps/photon/photon-app-editor` (`EditorCore` + `photon-editor` + `photon-canvas`)
   - `crates/apps/notes` (`NotesCore` + `notes` + `notesctl` — reference minimal app on `fw-*` with zero `photon-*` deps)

## Core architectural invariants (non-negotiable)
1. **Automations-First & Headless-Capable Core**:
   - Each app owns its own lightweight `AppCore` (`fw-runtime`) as the single authoritative owner of domain state (`revision`), config, and operations (`ViewerCore`, `ManagerCore`, `EditorCore`, `NotesCore`).
   - `photon-manager --headless` (and `photon --headless`) must run domain, IPC, and journal operations with zero GUI/GPU dependencies.
2. **Single Command Gateway**:
   - GUI (`iced`), CLI (`photonctl`), Local IPC (`photon-ipc` / `fw-ipc`), MCP (`photonctl mcp`), and Replay (`debug replay`) all submit typed `Envelope { id, protocol, source, expected_revision, command }` to `CoreRuntime`.
   - Queries (`Query`) are read-only; Events (`Event`) are sequenced notifications (`seq_id`).
3. **UI is the presentation edge**:
   - `iced` owns only ephemeral presentation state (viewport zoom/pan, hover, scroll, focus, animation, GPU texture cache).
   - Domain mutations always emit a typed `Command` to `CoreHandle`; never mutate files or domain state inside `iced::view()` or `update()` directly.
   - Offload blocking image/thumbnail decodes onto an external Tokio runtime (`BG_RUNTIME`), never `iced`'s internal runtime, so `iced::exit()` never hangs.
4. **Pure-Rust i18n (`photon-i18n`)**:
   - Core emits `UiText { code, args }` only — never translated UI sentences.
   - Every `en-US` key and placeholder must match `vi-VN` (enforced by `cargo test -p photon-i18n`).
5. **Non-destructive editing & Profile isolation**:
   - Dev, Prod, and Sandbox (`PHOTON_SANDBOX_DIR` / `NOTES_SANDBOX_DIR`) never share config, logs, journals, or IPC sockets.
   - Overwriting an image in Editor always stages to a temp file and keeps numbered backups (`file~`).

## Verification
- `just fmt-check && just test` before finishing any change.
- Never claim real-window or GPU behavior works unless actually launched and inspected.
